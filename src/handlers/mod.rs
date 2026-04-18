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
        • Gestionar tus suscripciones y pagos\n\n\
        Usa /help para ver los comandos disponibles.",
        state.config.bot.name,
        user.first_name
    );

    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Html)
        .await?;
    Ok(())
}

pub async fn handle_help(bot: Bot, msg: Message) -> anyhow::Result<()> {
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
        .await?;
    Ok(())
}

// ===== USER SEARCHES =====

pub async fn handle_busquedas(bot: Bot, msg: Message, state: Arc<BotState>) -> anyhow::Result<()> {
    let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
    let configs = state.db.get_user_search_configs(user_id).await?;

    if configs.is_empty() {
        bot.send_message(
            msg.chat.id,
            "No tienes búsquedas configuradas. Usa /nueva_busqueda para añadir una.",
        )
        .await?;
        return Ok(());
    }

    let mut text = "🔍 <b>Tus búsquedas</b>\n\n".to_string();
    for c in &configs {
        text.push_str(&format!(
            "• <b>{}</b> (ID: <code>{}</code>)\n  URL: {}\n  Tipo: {}\n  Notificación: {}\n  Palabras clave: {}\n\n",
            c.name,
            c.id,
            c.url,
            c.search_type,
            c.notify_mode,
            c.keywords.as_deref().unwrap_or("-")
        ));
    }

    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Html)
        .disable_web_page_preview(true)
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
        .create_search_config(user_id, name, url, search_type, keywords, css_selector, Some("daily"))
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
            "Uso: /eliminar_busqueda <id>\n\nUsa /busquedas para ver los IDs.",
        )
        .await?;
    }
    Ok(())
}

pub async fn handle_informe(bot: Bot, msg: Message, state: Arc<BotState>) -> anyhow::Result<()> {
    let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
    let results = state.db.get_unnotified_results(user_id).await?;

    if results.is_empty() {
        bot.send_message(msg.chat.id, "📊 No hay resultados nuevos pendientes.").await?;
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

    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Html)
        .disable_web_page_preview(true)
        .await?;

    state.db.mark_results_notified(ids).await?;
    Ok(())
}

// ===== SUBSCRIPTION / PAYMENTS =====

pub async fn handle_suscribirse(
    bot: Bot,
    msg: Message,
    state: Arc<BotState>,
) -> anyhow::Result<()> {
    let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
    let configs = state.db.get_user_search_configs(user_id).await?;
    let pricing = state.db.get_pricing().await?;
    let monthly = configs.len() as f64 * pricing.price_per_search_eur;

    let keyboard = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        "💳 Pagar suscripción",
        format!("pay:{}:{}", configs.len(), monthly),
    )]]);

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
    Ok(())
}

// ===== ADMIN =====

pub async fn handle_admin_precio(
    bot: Bot,
    msg: Message,
    state: Arc<BotState>,
) -> anyhow::Result<()> {
    let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
    if !is_admin(&state, user_id) {
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
    Ok(())
}

pub async fn handle_admin_usuarios(
    bot: Bot,
    msg: Message,
    state: Arc<BotState>,
) -> anyhow::Result<()> {
    let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
    if !is_admin(&state, user_id) {
        bot.send_message(msg.chat.id, "⚠️ Solo admin.").await?;
        return Ok(());
    }

    let users = state.db.get_all_users().await?;
    let mut text = format!("👥 <b>Usuarios ({})</b>\n\n", users.len());
    for u in users.iter().take(20) {
        text.push_str(&format!(
            "• {} {} (@{}) - admin:{}\n",
            u.telegram_id,
            u.first_name.as_deref().unwrap_or(""),
            u.username.as_deref().unwrap_or("-"),
            if u.is_admin { "✓" } else { "✗" }
        ));
    }

    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Html)
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
                    <b>Notificación:</b> {}\n\n\
                    ¿Guardar?",
                    name.as_deref().unwrap_or("-"),
                    url.as_deref().unwrap_or("-"),
                    search_type.as_deref().unwrap_or("-"),
                    keywords.as_deref().unwrap_or("-"),
                    css_selector.as_deref().unwrap_or("(ninguno)"),
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

            if url.is_empty() {
                bot.answer_callback_query(q.id).text("❌ Faltan datos.").await?;
                return Ok(());
            }

            match state
                .db
                .create_search_config(user_id, name, url, search_type, keywords, css_selector, data.notify_mode.as_deref())
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
    InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        "❌ Cancelar wizard",
        "wiz:cancel",
    )]])
}
