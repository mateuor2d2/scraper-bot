use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode};
use teloxide::RequestError;

use crate::config::Config;
use crate::db::Db;
use crate::payments::StripeClient;
use crate::wizard::{self, WizardData, WizardStep};

fn map_anyhow<E: std::fmt::Display>(e: E) -> RequestError {
    RequestError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
}

#[derive(Clone)]
pub struct BotState {
    pub db: Arc<Db>,
    pub config: Arc<Config>,
}

// ===== START / HELP =====

pub async fn handle_start(bot: Bot, msg: Message, state: Arc<BotState>) -> anyhow::Result<()> {
    let user = msg.from().ok_or_else(|| anyhow::anyhow!("No user"))?;
    let _db_user = state
        .db
        .get_or_create_user(
            user.id.0 as i64,
            user.username.as_deref(),
            Some(user.first_name.as_str()),
            user.last_name.as_deref(),
        )
        .await?;

    let is_admin = state.config.bot.admins.contains(&(user.id.0 as i64));
    if is_admin {
        let _ = state.db.set_user_admin(user.id.0 as i64, true).await;
    }

    let text = format!(
        "🔍 <b>{}</b>\n\n\
        Bienvenido, {}.\n\n\
        Con este bot puedes:\n\
        • Configurar búsquedas automatizadas en portales públicos\n\
        • Recibir informes diarios con nuevas oportunidades\n\
        • Gestionar tus suscripciones y pagos",
        state.config.bot.name,
        user.first_name
    );

    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(main_menu_keyboard(is_admin))
        .await?;
    Ok(())
}

pub async fn handle_help(bot: Bot, msg: Message, state: Arc<BotState>) -> anyhow::Result<()> {
    let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
    let is_admin = state.config.bot.admins.contains(&user_id);
    let text = "📚 <b>Comandos disponibles</b>\n\n\
    <b>Usuario</b>\n\
    /start - Iniciar el bot\n\
    /help - Mostrar ayuda\n\
    /busquedas - Ver tus búsquedas configuradas\n\
    /nueva_busqueda - Crear una nueva búsqueda\n\
    /eliminar_busqueda - Borrar una búsqueda\n\
    /suscribirse - Gestionar tu suscripción\n\
    /informe - Ver informe manual\n\n\
    <b>Admin</b>\n\
    /admin_precio - Cambiar precio por búsqueda\n\
    /admin_usuarios - Listar usuarios";

    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(main_menu_keyboard(is_admin))
        .await?;
    Ok(())
}

// ===== USER SEARCHES =====

pub async fn handle_busquedas(bot: Bot, msg: Message, state: Arc<BotState>) -> anyhow::Result<()> {
    let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
    let is_admin = state.config.bot.admins.contains(&user_id);
    let configs = state.db.get_user_search_configs(user_id).await?;

    if configs.is_empty() {
        bot.send_message(
            msg.chat.id,
            "No tienes búsquedas configuradas. Pulsa ➕ Nueva búsqueda para añadir una.",
        )
        .reply_markup(main_menu_keyboard(is_admin))
        .await?;
        return Ok(());
    }

    bot.send_message(msg.chat.id, "🔍 <b>Tus búsquedas</b>")
        .parse_mode(ParseMode::Html)
        .await?;

    for c in &configs {
        let filters_display = c.filters.as_deref()
            .map(|f| crate::filters::FilterConfig::parse(f).to_display_string())
            .unwrap_or_else(|| "(sin filtros)".to_string());
        let text = format!(
            "• <b>{}</b> (ID: <code>{}</code>)\n  Tipo: {}\n  Notif: {}\n  Keywords: {}\n  Filtros: {}",
            c.name,
            c.id,
            c.search_type,
            c.notify_mode,
            c.keywords.as_deref().unwrap_or("-"),
            filters_display
        );
        bot.send_message(msg.chat.id, text)
            .parse_mode(ParseMode::Html)
            .reply_markup(busquedas_action_keyboard(c.id))
            .disable_web_page_preview(true)
            .await?;
    }

    bot.send_message(msg.chat.id, "📋 Usa los botones 🗑️ para eliminar, o el menú para otras acciones.")
        .reply_markup(main_menu_keyboard(is_admin))
        .await?;
    Ok(())
}

pub async fn handle_nueva_busqueda_inline(
    bot: Bot,
    msg: Message,
    _state: Arc<BotState>,
) -> anyhow::Result<()> {
    let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
    wizard::start_wizard(user_id);
    bot.send_message(
        msg.chat.id,
        "📝 <b>Nueva búsqueda</b>\n\nPaso 1/6: Escribe un <b>nombre</b> para identificar esta búsqueda.",
    )
    .parse_mode(ParseMode::Html)
    .reply_markup(cancel_keyboard())
    .await?;
    Ok(())
}

pub async fn handle_nueva_busqueda_fast(
    bot: Bot,
    msg: Message,
    state: Arc<BotState>,
    text: String,
) -> anyhow::Result<()> {
    let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
    let parts: Vec<&str> = text.split('|').map(|s| s.trim()).collect();
    if parts.len() < 4 {
        bot.send_message(
            msg.chat.id,
            "⚠️ Formato incorrecto.\n\nUso:\n\
            <code>/nueva_busqueda nombre | url | tipo | palabras clave | selector CSS</code>",
        )
        .parse_mode(ParseMode::Html)
        .await?;
        return Ok(());
    }

    let name = parts[0];
    let url = parts[1];
    let search_type = parts[2];
    let keywords = if parts[3].is_empty() { None } else { Some(parts[3]) };
    let css_selector = parts.get(4).filter(|s| !s.is_empty()).copied();

    let id = state
        .db
        .create_search_config(user_id, name, url, search_type, keywords, css_selector, Some("daily"), None)
        .await?;
    recalc_subscription(&state.db, user_id).await?;

    bot.send_message(
        msg.chat.id,
        format!("✅ Búsqueda '<b>{}</b>' creada con ID <code>{}</code>.", name, id),
    )
    .parse_mode(ParseMode::Html)
    .await?;
    Ok(())
}

pub async fn handle_eliminar_busqueda(
    bot: Bot,
    msg: Message,
    state: Arc<BotState>,
) -> anyhow::Result<()> {
    let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
    let is_admin = state.config.bot.admins.contains(&user_id);
    let text = msg.text().unwrap_or("").trim();
    let id_str = text.trim_start_matches("/eliminar_busqueda").trim();

    if let Ok(id) = id_str.parse::<i64>() {
        if state.db.delete_search_config(id, user_id).await? {
            recalc_subscription(&state.db, user_id).await?;
            bot.send_message(msg.chat.id, "✅ Búsqueda eliminada.").await?;
        } else {
            bot.send_message(msg.chat.id, "❌ No se encontró la búsqueda o no te pertenece.").await?;
        }
    } else {
        bot.send_message(
            msg.chat.id,
            "Uso: /eliminar_busqueda <id>\n\nUsa 🔍 Mis búsquedas para ver los IDs.",
        )
        .await?;
    }
    bot.send_message(msg.chat.id, "📋 Vuelve al menú principal:")
        .reply_markup(main_menu_keyboard(is_admin))
        .await?;
    Ok(())
}

pub async fn handle_informe(bot: Bot, msg: Message, state: Arc<BotState>) -> anyhow::Result<()> {
    let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
    let is_admin = state.config.bot.admins.contains(&user_id);
    let results = state.db.get_unnotified_results(user_id).await?;

    if results.is_empty() {
        bot.send_message(msg.chat.id, "📊 No hay resultados nuevos pendientes.")
            .reply_markup(main_menu_keyboard(is_admin))
            .await?;
        return Ok(());
    }

    let mut text = format!("📊 <b>Informe manual</b>\n\n<b>{}</b> resultados nuevos:\n\n", results.len());
    let mut ids = Vec::new();

    for result in &results[..std::cmp::min(10, results.len())] {
        ids.push(result.id);
        let title = result.title.as_deref().unwrap_or("Sin título");
        let url = result.url.as_deref().unwrap_or("#");
        text.push_str(&format!(
            "🔍 <b>{}</b>\n<a href=\"{}\">{}</a>\n\n",
            result.config_name, url, title
        ));
    }

    if results.len() > 10 {
        text.push_str(&format!("...y {} más\n", results.len() - 10));
    }

    let refresh_kb = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback("🔄 Refrescar informe", "menu:informe")],
    ]);

    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Html)
        .disable_web_page_preview(true)
        .reply_markup(refresh_kb)
        .await?;

    state.db.mark_results_notified(ids).await?;

    bot.send_message(msg.chat.id, "✅ Informe generado. Usa 🔄 para refrescar o el menú para otras acciones.")
        .reply_markup(main_menu_keyboard(is_admin))
        .await?;
    Ok(())
}

// ===== SUBSCRIPTION / PAYMENTS =====

pub async fn handle_suscribirse(
    bot: Bot,
    msg: Message,
    state: Arc<BotState>,
) -> anyhow::Result<()> {
    let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
    let is_admin = state.config.bot.admins.contains(&user_id);
    let configs = state.db.get_user_search_configs(user_id).await?;
    let pricing = state.db.get_pricing().await?;
    let monthly = configs.len() as f64 * pricing.price_per_search_eur;

    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(
            "💳 Pagar suscripción",
            format!("pay:{}:{}", configs.len(), monthly),
        )],
    ]);

    bot.send_message(
        msg.chat.id,
        format!(
            "💰 <b>Tu suscripción</b>\n\n\
            Búsquedas activas: {}\n\
            Precio por búsqueda: {:.2}€\n\
            <b>Total mensual: {:.2}€</b>",
            configs.len(),
            pricing.price_per_search_eur,
            monthly
        ),
    )
    .parse_mode(ParseMode::Html)
    .reply_markup(keyboard)
    .await?;

    bot.send_message(msg.chat.id, "📋 Gestiona tu suscripción o vuelve al menú principal.")
        .reply_markup(main_menu_keyboard(is_admin))
        .await?;
    Ok(())
}

// ===== ADMIN =====

pub async fn handle_admin_precio(
    bot: Bot,
    msg: Message,
    state: Arc<BotState>,
) -> anyhow::Result<()> {
    let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
    let is_admin = state.config.bot.admins.contains(&user_id);
    if !is_admin {
        bot.send_message(msg.chat.id, "⚠️ Solo admin.").await?;
        return Ok(());
    }

    let text = msg.text().unwrap_or("").trim();
    let price_str = text.trim_start_matches("/admin_precio").trim();

    if let Ok(price) = price_str.parse::<f64>() {
        state.db.set_pricing(price).await?;
        bot.send_message(
            msg.chat.id,
            format!("✅ Precio por búsqueda actualizado a <b>{:.2}€</b>", price),
        )
        .parse_mode(ParseMode::Html)
        .await?;
    } else {
        let pricing = state.db.get_pricing().await?;
        bot.send_message(
            msg.chat.id,
            format!(
                "Precio actual: <b>{:.2}€</b>\n\nUso: /admin_precio 7.50",
                pricing.price_per_search_eur
            ),
        )
        .parse_mode(ParseMode::Html)
        .await?;
    }
    bot.send_message(msg.chat.id, "📋 Vuelve al menú principal:")
        .reply_markup(main_menu_keyboard(true))
        .await?;
    Ok(())
}

pub async fn handle_admin_usuarios(
    bot: Bot,
    msg: Message,
    state: Arc<BotState>,
) -> anyhow::Result<()> {
    let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
    let is_admin = state.config.bot.admins.contains(&user_id);
    if !is_admin {
        bot.send_message(msg.chat.id, "⚠️ Solo admin.").await?;
        return Ok(());
    }

    let users = state.db.get_all_users().await?;
    let mut text = format!("👤 <b>{} usuarios</b>\n\n", users.len());
    for u in &users {
        text.push_str(&format!(
            "• {} {} (@{}) — admin:{}\n",
            u.telegram_id,
            u.first_name.as_deref().unwrap_or(""),
            u.username.as_deref().unwrap_or("-"),
            if u.is_admin { "✓" } else { "✗" }
        ));
    }
    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(main_menu_keyboard(true))
        .await?;
    Ok(())
}

// ===== CALLBACKS =====

pub async fn handle_callback(
    bot: Bot,
    q: CallbackQuery,
    state: Arc<BotState>,
) -> Result<(), RequestError> {
    let data = q.data.as_deref().unwrap_or("");
    let user_id = q.from.id.0 as i64;
    let chat_id = q.message.as_ref().map(|m| m.chat.id);
    let _message_id = q.message.as_ref().map(|m| m.id);

    if data.starts_with("wiz:type:") {
        let tipo = data.strip_prefix("wiz:type:").unwrap_or("generic_html");
        if let Some(wiz) = wizard::get_wizard_state(user_id) {
            let new_data = WizardData {
                search_type: Some(tipo.to_string()),
                ..wiz.data.clone()
            };
            wizard::update_wizard_data(user_id, new_data);
            wizard::set_wizard_step(user_id, WizardStep::AskKeywords);
            if let Some(cid) = chat_id {
                bot.send_message(
                    cid,
                    "✅ Tipo seleccionado.\n\nPaso 4/6: Escribe las <b>palabras clave</b> separadas por comas (o envía un punto <code>.</code> para omitir).",
                )
                .parse_mode(ParseMode::Html)
                .reply_markup(cancel_keyboard())
                .await?;
            }
        }
        bot.answer_callback_query(q.id).await?;
        return Ok(());
    }

        if data.starts_with("wiz:notify:") {
        let mode = data.strip_prefix("wiz:notify:").unwrap_or("daily");
        if let Some(wiz) = wizard::get_wizard_state(user_id) {
            let new_data = WizardData {
                notify_mode: Some(mode.to_string()),
                ..wiz.data.clone()
            };
            let name = new_data.name.clone();
            let url = new_data.url.clone();
            let search_type = new_data.search_type.clone();
            let keywords = new_data.keywords.clone();
            let css_selector = new_data.css_selector.clone();
            let filters = new_data.filters.clone();
            let notify_mode = new_data.notify_mode.clone();
            wizard::update_wizard_data(user_id, new_data);
            wizard::set_wizard_step(user_id, WizardStep::Confirm);
            if let Some(cid) = chat_id {
                let summary = format!(
                    "📋 <b>Resumen de la búsqueda</b>\n\n\
                    <b>Nombre:</b> {}\n\
                    <b>URL:</b> {}\n\
                    <b>Tipo:</b> {}\n\
                    <b>Palabras clave:</b> {}\n\
                    <b>Selector CSS:</b> {}\n\
                    <b>Filtros:</b> {}\n\
                    <b>Notificación:</b> {}\n\n\
                    ¿Guardar?",
                    name.as_deref().unwrap_or("-"),
                    url.as_deref().unwrap_or("-"),
                    search_type.as_deref().unwrap_or("-"),
                    keywords.as_deref().unwrap_or("-"),
                    css_selector.as_deref().unwrap_or("(ninguno)"),
                    filters.as_deref().unwrap_or("(ninguno)"),
                    notify_mode.as_deref().unwrap_or("daily")
                );
                bot.send_message(cid, summary)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(InlineKeyboardMarkup::new(vec![
                        vec![
                            InlineKeyboardButton::callback("✅ Guardar", "wiz:confirm:yes"),
                            InlineKeyboardButton::callback("❌ Cancelar", "wiz:confirm:no"),
                        ],
                    ]))
                    .await?;
            }
        }
        bot.answer_callback_query(q.id).await?;
        return Ok(());
    }

    if data == "wiz:cancel" {
        wizard::clear_wizard(user_id);
        bot.answer_callback_query(q.id).text("❌ Wizard cancelado.").await?;
        if let Some(cid) = chat_id {
            bot.send_message(cid, "Wizard cancelado. Puedes empezar de nuevo con /nueva_busqueda.").await?;
        }
        return Ok(());
    }

    if data == "wiz:confirm:no" {
        wizard::clear_wizard(user_id);
        bot.answer_callback_query(q.id).text("❌ Cancelado.").await?;
        if let Some(cid) = chat_id {
            bot.send_message(cid, "Búsqueda descartada.").await?;
        }
        return Ok(());
    }

    if data == "wiz:confirm:yes" {
        if let Some(wiz) = wizard::get_wizard_state(user_id) {
            let data = wiz.data.clone();
            wizard::clear_wizard(user_id);

            let name = data.name.as_deref().unwrap_or("Sin nombre");
            let url = data.url.as_deref().unwrap_or("");
            let search_type = data.search_type.as_deref().unwrap_or("generic_html");
            let keywords = data.keywords.as_deref();
            let css_selector = data.css_selector.as_deref();
            let filters = data.filters.as_deref();

            if url.is_empty() {
                bot.answer_callback_query(q.id).text("❌ Faltan datos.").await?;
                return Ok(());
            }

            match state
                .db
                .create_search_config(user_id, name, url, search_type, keywords, css_selector, data.notify_mode.as_deref(), filters)
                .await
                .map_err(|e| map_anyhow(e))
            {
                Ok(id) => {
                    let _ = recalc_subscription(&state.db, user_id).await;
                    bot.answer_callback_query(q.id).text("✅ Guardada.").await?;
                    if let Some(cid) = chat_id {
                        bot.send_message(
                            cid,
                            format!("✅ Búsqueda '<b>{}</b>' creada con ID <code>{}</code>.", name, id),
                        )
                        .parse_mode(ParseMode::Html)
                        .await?;
                    }
                }
                Err(e) => {
                    bot.answer_callback_query(q.id).text("❌ Error guardando.").await?;
                    tracing::error!("Error creando búsqueda: {}", e);
                }
            }
        }
        return Ok(());
    }

    // ===== MENU PRINCIPAL CALLBACKS =====
    if data.starts_with("menu:") {
        let action = data.strip_prefix("menu:").unwrap_or("");
        bot.answer_callback_query(q.id).await?;
        if let Some(cid) = chat_id {
            match action {
                "busquedas" => {
                    let _ = handle_busquedas(bot.clone(), fake_msg(cid, user_id), state.clone()).await;
                }
                "nueva" => {
                    let _ = handle_nueva_busqueda_inline(bot.clone(), fake_msg(cid, user_id), state.clone()).await;
                }
                "informe" => {
                    let _ = handle_informe(bot.clone(), fake_msg(cid, user_id), state.clone()).await;
                }
                "suscribirse" => {
                    let _ = handle_suscribirse(bot.clone(), fake_msg(cid, user_id), state.clone()).await;
                }
                "help" => {
                    let _ = handle_help(bot.clone(), fake_msg(cid, user_id), state.clone()).await;
                }
                "start" => {
                    let _ = handle_start(bot.clone(), fake_msg(cid, user_id), state.clone()).await;
                }
                "admin_usuarios" => {
                    let _ = handle_admin_usuarios(bot.clone(), fake_msg(cid, user_id), state.clone()).await;
                }
                "admin_precio" => {
                    let _ = handle_admin_precio(bot.clone(), fake_msg(cid, user_id), state.clone()).await;
                }
                _ => {}
            }
        }
        return Ok(());
    }
    // ===== EDIT CALLBACKS =====
    if data.starts_with("edit:") {
        let id_str = data.strip_prefix("edit:").unwrap_or("");
        if let Ok(id) = id_str.parse::<i64>() {
            if let Ok(Some(config)) = state.db.get_search_config(id).await {
                if config.telegram_id != user_id {
                    bot.answer_callback_query(q.id).text("❌ No te pertenece esta búsqueda.").await?;
                    return Ok(());
                }
                let current_data = wizard::WizardData {
                    name: Some(config.name),
                    url: Some(config.url),
                    search_type: Some(config.search_type),
                    keywords: config.keywords,
                    css_selector: config.css_selector,
                    notify_mode: Some(config.notify_mode),
                    filters: config.filters,
                };
                wizard::start_edit_wizard(user_id, id, current_data.clone());
                bot.answer_callback_query(q.id).text("✏️ Modo edición activado.").await?;
                if let Some(cid) = chat_id {
                    let summary = format!(
                        "✏️ <b>Editar búsqueda</b>\n\n\
                        <b>Nombre:</b> {}\n\
                        <b>URL:</b> {}\n\
                        <b>Tipo:</b> {}\n\
                        <b>Palabras clave:</b> {}\n\
                        <b>Selector CSS:</b> {}\n\
                        <b>Notificación:</b> {}\n\n\
                        ¿Qué campo quieres modificar?",
                        current_data.name.as_deref().unwrap_or("-"),
                        current_data.url.as_deref().unwrap_or("-"),
                        current_data.search_type.as_deref().unwrap_or("-"),
                        current_data.keywords.as_deref().unwrap_or("-"),
                        current_data.css_selector.as_deref().unwrap_or("(ninguno)"),
                        current_data.notify_mode.as_deref().unwrap_or("daily")
                    );
                    bot.send_message(cid, summary)
                        .parse_mode(ParseMode::Html)
                        .reply_markup(edit_field_keyboard())
                        .await?;
                }
            } else {
                bot.answer_callback_query(q.id).text("❌ Búsqueda no encontrada.").await?;
            }
        }
        return Ok(());
    }

    if data.starts_with("editfield:") {
        let field = data.strip_prefix("editfield:").unwrap_or("");
        if field == "menu" {
            if let Some(edit) = wizard::get_edit_state(user_id) {
                wizard::set_edit_step(user_id, wizard::EditStep::ChooseField);
                bot.answer_callback_query(q.id).await?;
                if let Some(cid) = chat_id {
                    let summary = format!(
                        "✏️ <b>Editar búsqueda</b>\n\n\
                        <b>Nombre:</b> {}\n\
                        <b>URL:</b> {}\n\
                        <b>Tipo:</b> {}\n\
                        <b>Palabras clave:</b> {}\n\
                        <b>Selector CSS:</b> {}\n\
                        <b>Notificación:</b> {}\n\n\
                        ¿Qué campo quieres modificar?",
                        edit.data.name.as_deref().unwrap_or("-"),
                        edit.data.url.as_deref().unwrap_or("-"),
                        edit.data.search_type.as_deref().unwrap_or("-"),
                        edit.data.keywords.as_deref().unwrap_or("-"),
                        edit.data.css_selector.as_deref().unwrap_or("(ninguno)"),
                        edit.data.notify_mode.as_deref().unwrap_or("daily")
                    );
                    bot.send_message(cid, summary)
                        .parse_mode(ParseMode::Html)
                        .reply_markup(edit_field_keyboard())
                        .await?;
                }
            }
            return Ok(());
        }
        if let Some(edit) = wizard::get_edit_state(user_id) {
            let new_step = match field {
                "name" => wizard::EditStep::EditName,
                "url" => wizard::EditStep::EditUrl,
                "keywords" => wizard::EditStep::EditKeywords,
                "selector" => wizard::EditStep::EditSelector,
                "notify" => wizard::EditStep::EditNotifyMode,
                "filters" => wizard::EditStep::EditFilters,
                _ => wizard::EditStep::ChooseField,
            };
            wizard::set_edit_step(user_id, new_step);
            let prompt = match field {
                "name" => "✏️ Escribe el nuevo <b>nombre</b> para esta búsqueda:",
                "url" => "✏️ Escribe la nueva <b>URL</b>:",
                "keywords" => "✏️ Escribe las nuevas <b>palabras clave</b> separadas por comas (o un punto <code>.</code> para omitir):",
                "selector" => "✏️ Escribe el nuevo <b>selector CSS</b> (o un punto <code>.</code> para omitir):",
                "notify" => "✏️ Elige el modo de notificación:",
                "filters" => "✏️ Escribe los <b>filtros</b>.\n\nFormato:\n• <code>+Baleares,+Mallorca</code> (solo resultados que contengan ambas)\n• <code>-Cáceres,-expirado</code> (excluir)\n• <code>+Baleares,-Cáceres</code> (combinado)\n\nEscribe un punto <code>.</code> para borrar filtros:",
                _ => "✏️ Elige una opción:",
            };
            bot.answer_callback_query(q.id).await?;
            if let Some(cid) = chat_id {
                let kb = if field == "notify" {
                    InlineKeyboardMarkup::new(vec![
                        vec![
                            InlineKeyboardButton::callback("📅 Diaria", "editnotify:daily"),
                            InlineKeyboardButton::callback("⚡ Inmediata", "editnotify:immediate"),
                        ],
                        vec![
                            InlineKeyboardButton::callback("◀️ Cancelar edición", "editcancel"),
                            InlineKeyboardButton::callback("🏠 Inicio", "menu:start"),
                        ],
                    ])
                } else {
                    InlineKeyboardMarkup::new(vec![
                        vec![InlineKeyboardButton::callback("◀️ Cancelar edición", "editcancel")],
                        vec![InlineKeyboardButton::callback("🏠 Inicio", "menu:start")],
                    ])
                };
                bot.send_message(cid, prompt)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(kb)
                    .await?;
            }
        }
        return Ok(());
    }

    if data.starts_with("editnotify:") {
        let mode = data.strip_prefix("editnotify:").unwrap_or("daily");
        if let Some(edit) = wizard::get_edit_state(user_id) {
            let mut new_data = edit.data.clone();
            new_data.notify_mode = Some(mode.to_string());
            wizard::update_edit_data(user_id, new_data.clone());
            wizard::set_edit_step(user_id, wizard::EditStep::Confirm);
            if let Some(cid) = chat_id {
                let summary = format!(
                    "📋 <b>Resumen de cambios</b>\n\n\
                    <b>Nombre:</b> {}\n\
                    <b>URL:</b> {}\n\
                    <b>Tipo:</b> {}\n\
                    <b>Palabras clave:</b> {}\n\
                    <b>Selector CSS:</b> {}\n\
                    <b>Notificación:</b> {}\n\n\
                    ¿Guardar cambios?",
                    new_data.name.as_deref().unwrap_or("-"),
                    new_data.url.as_deref().unwrap_or("-"),
                    new_data.search_type.as_deref().unwrap_or("-"),
                    new_data.keywords.as_deref().unwrap_or("-"),
                    new_data.css_selector.as_deref().unwrap_or("(ninguno)"),
                    new_data.notify_mode.as_deref().unwrap_or("daily")
                );
                bot.send_message(cid, summary)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(InlineKeyboardMarkup::new(vec![
                        vec![
                            InlineKeyboardButton::callback("✅ Guardar", "editsave"),
                            InlineKeyboardButton::callback("❌ Descartar", "editcancel"),
                        ],
                        vec![
                            InlineKeyboardButton::callback("◀️ Atrás", "menu:busquedas"),
                            InlineKeyboardButton::callback("🏠 Inicio", "menu:start"),
                        ],
                    ]))
                    .await?;
            }
        }
        bot.answer_callback_query(q.id).await?;
        return Ok(());
    }

    if data == "editsave" {
        if let Some(edit) = wizard::get_edit_state(user_id) {
            let data = edit.data.clone();
            match state.db.update_search_config(
                edit.config_id,
                user_id,
                data.name.as_deref(),
                data.url.as_deref(),
                data.search_type.as_deref(),
                data.keywords.as_deref(),
                data.css_selector.as_deref(),
                data.notify_mode.as_deref(),
                data.filters.as_deref(),
            ).await {
                Ok(true) => {
                    wizard::clear_editor(user_id);
                    bot.answer_callback_query(q.id).text("✅ Cambios guardados.").await?;
                    if let Some(cid) = chat_id {
                        bot.send_message(cid, "✅ Búsqueda actualizada correctamente.")
                            .reply_markup(back_home_keyboard())
                            .await?;
                    }
                }
                Ok(false) => {
                    bot.answer_callback_query(q.id).text("❌ No se encontró la búsqueda.").await?;
                }
                Err(e) => {
                    bot.answer_callback_query(q.id).text("❌ Error guardando.").await?;
                    tracing::error!("Error actualizando búsqueda: {}", e);
                }
            }
        }
        return Ok(());
    }

    if data == "editcancel" {
        wizard::clear_editor(user_id);
        bot.answer_callback_query(q.id).text("❌ Edición cancelada.").await?;
        if let Some(cid) = chat_id {
            bot.send_message(cid, "Edición cancelada. Los cambios no se han guardado.")
                .reply_markup(back_home_keyboard())
                .await?;
        }
        return Ok(());
    }

    if data.starts_with("del:") {
        let id_str = data.strip_prefix("del:").unwrap_or("");
        if let Ok(id) = id_str.parse::<i64>() {
            if state.db.delete_search_config(id, user_id).await.map_err(|e| map_anyhow(e))? {
                let _ = recalc_subscription(&state.db, user_id).await;
                bot.answer_callback_query(q.id).text("✅ Búsqueda eliminada.").await?;
                if let Some(cid) = chat_id {
                    let _ = bot.send_message(cid, "✅ Búsqueda eliminada. Usa 🔍 Mis búsquedas para ver el listado actualizado.").await;
                }
            } else {
                bot.answer_callback_query(q.id).text("❌ No se encontró o no te pertenece.").await?;
            }
        }
        return Ok(());
    }
    if data.starts_with("pay:") {
        let parts: Vec<&str> = data.split(':').collect();
        if parts.len() >= 3 {
            if let (Ok(searches), Ok(price)) = (parts[1].parse::<i32>(), parts[2].parse::<f64>()) {
                let stripe = StripeClient::new(Arc::new(state.config.stripe.clone()));
                match stripe.create_subscription_checkout(user_id, searches, price).await.map_err(|e| map_anyhow(e)) {
                    Ok(url) => {
                        bot.answer_callback_query(q.id).text("💳 Abre el enlace...").await?;
                        if let Some(cid) = chat_id {
                            bot.send_message(
                                cid,
                                format!("💳 <b>Pago seguro por Stripe</b>\n\nHaz clic para completar tu suscripción:\n<a href=\"{}\">Pagar {:.2}€</a>", url, price),
                            )
                            .parse_mode(ParseMode::Html)
                            .await?;
                        }
                    }
                    Err(e) => {
                        bot.answer_callback_query(q.id).text("❌ Error generando pago.").await?;
                        tracing::error!("Stripe error: {}", e);
                    }
                }
            }
        }
        return Ok(());
    }

    Ok(())
}

// ===== HELPERS =====

fn is_admin(state: &BotState, user_id: i64) -> bool {
    state.config.bot.admins.contains(&user_id)
}

async fn recalc_subscription(db: &Db, telegram_id: i64) -> anyhow::Result<()> {
    let configs = db.get_user_search_configs(telegram_id).await?;
    let pricing = db.get_pricing().await?;
    let monthly = configs.len() as f64 * pricing.price_per_search_eur;
    db.upsert_subscription(
        telegram_id,
        configs.len() as i32,
        monthly,
        None,
        if monthly > 0.0 { "pending" } else { "active" },
    )
    .await?;
    Ok(())
}

fn cancel_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback("❌ Cancelar wizard", "wiz:cancel")],
        vec![
            InlineKeyboardButton::callback("◀️ Atrás", "menu:busquedas"),
            InlineKeyboardButton::callback("🏠 Inicio", "menu:start"),
        ],
    ])
}

fn back_home_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![
        InlineKeyboardButton::callback("◀️ Atrás", "menu:busquedas"),
        InlineKeyboardButton::callback("🏠 Inicio", "menu:start"),
    ]])
}

fn home_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![
        InlineKeyboardButton::callback("🏠 Inicio", "menu:start"),
    ]])
}

fn main_menu_keyboard(is_admin: bool) -> InlineKeyboardMarkup {
    let mut rows = vec![
        vec![
            InlineKeyboardButton::callback("🔍 Mis búsquedas", "menu:busquedas"),
            InlineKeyboardButton::callback("➕ Nueva búsqueda", "menu:nueva"),
        ],
        vec![
            InlineKeyboardButton::callback("📊 Informe", "menu:informe"),
            InlineKeyboardButton::callback("💰 Suscripción", "menu:suscribirse"),
        ],
        vec![
            InlineKeyboardButton::callback("❓ Ayuda", "menu:help"),
        ],
    ];
    if is_admin {
        rows.push(vec![
            InlineKeyboardButton::callback("👤 Admin: Usuarios", "menu:admin_usuarios"),
            InlineKeyboardButton::callback("💶 Admin: Precio", "menu:admin_precio"),
        ]);
    }
    InlineKeyboardMarkup::new(rows)
}

fn edit_field_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("🏷️ Nombre", "editfield:name"),
            InlineKeyboardButton::callback("🔗 URL", "editfield:url"),
        ],
        vec![
            InlineKeyboardButton::callback("🔑 Palabras clave", "editfield:keywords"),
            InlineKeyboardButton::callback("🎯 Selector CSS", "editfield:selector"),
        ],
        vec![
            InlineKeyboardButton::callback("🔔 Notificación", "editfield:notify"),
            InlineKeyboardButton::callback("🧪 Filtros", "editfield:filters"),
        ],
        vec![
            InlineKeyboardButton::callback("◀️ Atrás", "menu:busquedas"),
            InlineKeyboardButton::callback("🏠 Inicio", "menu:start"),
        ],
    ])
}

fn busquedas_action_keyboard(config_id: i64) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback(
                format!("✏️ Editar"),
                format!("edit:{}", config_id),
            ),
            InlineKeyboardButton::callback(
                format!("🗑️ Eliminar"),
                format!("del:{}", config_id),
            ),
        ],
        vec![
            InlineKeyboardButton::callback("◀️ Atrás", "menu:busquedas"),
            InlineKeyboardButton::callback("🏠 Inicio", "menu:start"),
        ],
    ])
}

fn fake_msg(chat_id: ChatId, user_id: i64) -> Message {
    use teloxide::types::{MessageId, MessageCommon, MessageKind, MediaKind, MediaText, User as TgUser, Chat as TgChat, ChatKind, ChatPrivate};
    Message {
        id: MessageId(1),
        date: chrono::Utc::now(),
        chat: TgChat {
            id: chat_id,
            kind: ChatKind::Private(ChatPrivate {
                username: None,
                first_name: Some("User".to_string()),
                last_name: None,
                emoji_status_custom_emoji_id: None,
                bio: None,
                has_private_forwards: None,
                has_restricted_voice_and_video_messages: None,
            }),
            photo: None,
            pinned_message: None,
            message_auto_delete_time: None,
            has_hidden_members: false,
            has_aggressive_anti_spam_enabled: false,
        },
        thread_id: None,
        via_bot: None,
        kind: MessageKind::Common(MessageCommon {
            from: Some(TgUser {
                id: teloxide::types::UserId(user_id as u64),
                is_bot: false,
                first_name: "User".to_string(),
                last_name: None,
                username: None,
                language_code: None,
                is_premium: false,
                added_to_attachment_menu: false,
            }),
            sender_chat: None,
            author_signature: None,
            forward: None,
            reply_to_message: None,
            edit_date: None,
            media_kind: MediaKind::Text(MediaText {
                text: "".to_string(),
                entities: vec![],
            }),
            reply_markup: None,
            is_topic_message: false,
            is_automatic_forward: false,
            has_protected_content: false,
        }),
    }
}
