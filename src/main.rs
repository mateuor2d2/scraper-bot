use std::sync::Arc;

use teloxide::prelude::*;
use teloxide::types::{BotCommand, InlineKeyboardButton, InlineKeyboardMarkup, ParseMode};
use teloxide::RequestError;

use scraper_bot::config::Config;
use scraper_bot::db::Db;
use scraper_bot::handlers::{
    handle_admin_precio, handle_admin_usuarios, handle_busquedas, handle_callback,
    handle_eliminar_busqueda, handle_help, handle_informe, handle_nueva_busqueda_inline,
    handle_start, handle_suscribirse, BotState,
};
use scraper_bot::scheduler;
use scraper_bot::webhook;
use scraper_bot::wizard::{self, WizardData, WizardState, WizardStep};
use scraper_bot::auto_learn;
use scraper_bot::handlers;
use scraper_bot::filters::FilterConfig;

fn map_anyhow<E: std::fmt::Display>(e: E) -> RequestError {
    RequestError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("Iniciando ScraperBot...");

    let config = Arc::new(Config::load()?);
    tokio::fs::create_dir_all("data").await.ok();

    let db = Arc::new(Db::new(&config.database.path()).await?);
    tracing::info!("Base de datos inicializada");

    let token = std::env::var("TELOXIDE_TOKEN")
        .expect("TELOXIDE_TOKEN debe estar configurado");
    let bot = Bot::new(token);

    let commands = vec![
        BotCommand::new("start", "Iniciar el bot"),
        BotCommand::new("help", "Mostrar ayuda"),
        BotCommand::new("busquedas", "Ver tus búsquedas"),
        BotCommand::new("nueva_busqueda", "Añadir búsqueda paso a paso"),
        BotCommand::new("eliminar_busqueda", "Eliminar búsqueda por ID"),
        BotCommand::new("suscribirse", "Gestionar suscripción"),
        BotCommand::new("informe", "Ver informe manual"),
        BotCommand::new("admin_precio", "[Admin] Cambiar precio por búsqueda"),
        BotCommand::new("admin_usuarios", "[Admin] Listar usuarios"),
    ];
    bot.set_my_commands(commands).await?;

    let state = Arc::new(BotState {
        db: Arc::clone(&db),
        config: Arc::clone(&config),
    });

    // Iniciar scheduler en background
    scheduler::start_scheduler(
        Arc::clone(&db),
        bot.clone(),
        config.scheduler.run_scrapes_interval_minutes,
        config.scheduler.daily_report_hour,
    );

    // Iniciar servidor webhook en background
    let webhook_db = Arc::clone(&db);
    let webhook_config = Arc::clone(&config);
    tokio::spawn(async move {
        if let Err(e) = webhook::start_webhook_server(webhook_db, webhook_config, 8080).await {
            tracing::error!("Error en servidor webhook: {}", e);
        }
    });

    tracing::info!("Bot listo: {}", config.bot.name);

    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .branch(teloxide::filter_command::<Command, _>().endpoint(handle_commands))
                .branch(dptree::endpoint(
                    |bot: Bot, msg: Message, state: Arc<BotState>| async move {
                        handle_text_messages(bot, msg, state).await
                    },
                )),
        )
        .branch(Update::filter_callback_query().endpoint(handle_callback));

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state])
        .default_handler(|upd| async move {
            tracing::warn!("Unhandled update: {:?}", upd);
        })
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}

#[derive(teloxide::macros::BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Comandos disponibles:")]
enum Command {
    #[command(description = "Iniciar el bot")]
    Start,
    #[command(description = "Mostrar ayuda")]
    Help,
    #[command(description = "Ver tus búsquedas")]
    Busquedas,
    #[command(description = "Añadir búsqueda")]
    NuevaBusqueda(String),
    #[command(description = "Eliminar búsqueda")]
    EliminarBusqueda(String),
    #[command(description = "Gestionar suscripción")]
    Suscribirse,
    #[command(description = "Ver informe manual")]
    Informe,
    #[command(description = "[Admin] Cambiar precio por búsqueda")]
    AdminPrecio(String),
    #[command(description = "[Admin] Listar usuarios")]
    AdminUsuarios,
}

async fn handle_commands(
    bot: Bot,
    msg: Message,
    cmd: Command,
    state: Arc<BotState>,
) -> Result<(), RequestError> {
    match cmd {
        Command::Start => handle_start(bot, msg, state).await.map_err(|e| map_anyhow(e)),
        Command::Help => handle_help(bot, msg, state.clone()).await.map_err(|e| map_anyhow(e)),
        Command::Busquedas => handle_busquedas(bot, msg, state).await.map_err(|e| map_anyhow(e)),
        Command::NuevaBusqueda(args) => {
            let args = args.trim();
            if args.is_empty() {
                handle_nueva_busqueda_inline(bot, msg, state).await.map_err(|e| map_anyhow(e))
            } else {
                handlers::handle_nueva_busqueda_fast(bot, msg, state, args.to_string()).await.map_err(|e| map_anyhow(e))
            }
        }
        Command::EliminarBusqueda(_) => handle_eliminar_busqueda(bot, msg, state).await.map_err(|e| map_anyhow(e)),
        Command::Suscribirse => handle_suscribirse(bot, msg, state).await.map_err(|e| map_anyhow(e)),
        Command::Informe => handle_informe(bot, msg, state).await.map_err(|e| map_anyhow(e)),
        Command::AdminPrecio(_) => handle_admin_precio(bot, msg, state).await.map_err(|e| map_anyhow(e)),
        Command::AdminUsuarios => handle_admin_usuarios(bot, msg, state).await.map_err(|e| map_anyhow(e)),
    }
}

async fn handle_text_messages(
    bot: Bot,
    msg: Message,
    state: Arc<BotState>,
) -> Result<(), RequestError> {
    let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
    let chat_id = msg.chat.id;

    if let Some(text) = msg.clone().text() {
        if let Some(wiz) = wizard::get_wizard_state(user_id) {
            return handle_wizard_text(bot, msg, state, wiz, text.to_string()).await.map_err(|e| map_anyhow(e));
        }

        if let Some(edit) = wizard::get_edit_state(user_id) {
            return handle_edit_text(bot, msg, state, edit, text.to_string()).await.map_err(|e| map_anyhow(e));
        }

        bot.send_message(
            chat_id,
            "🤖 No entiendo ese mensaje. Usa /help para ver los comandos disponibles.",
        )
        .await?;
    }

    Ok(())
}

async fn handle_wizard_text(
    bot: Bot,
    msg: Message,
    state: Arc<BotState>,
    wiz: WizardState,
    text: String,
) -> anyhow::Result<()> {
    use crate::wizard::{self, WizardStep};

    let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
    let chat_id = msg.chat.id;

    match wiz.step {
        WizardStep::AskName => {
            let data = WizardData {
                name: Some(text.clone()),
                ..wiz.data.clone()
            };
            wizard::update_wizard_data(user_id, data);
            wizard::set_wizard_step(user_id, WizardStep::AskUrl);
            bot.send_message(
                chat_id,
                format!("✅ Nombre: <b>{}</b>\n\nAhora envía la URL completa del portal donde quieres buscar.", text),
            )
            .parse_mode(ParseMode::Html)
            .reply_markup(cancel_keyboard())
            .await?;
        }
        WizardStep::AskUrl => {
            let data = WizardData {
                url: Some(text.clone()),
                ..wiz.data.clone()
            };
            wizard::update_wizard_data(user_id, data);
            wizard::set_wizard_step(user_id, WizardStep::AskType);
            bot.send_message(
                chat_id,
                "✅ URL guardada.\n\nSelecciona el tipo de scraping:",
            )
            .reply_markup(InlineKeyboardMarkup::new(vec![
                vec![InlineKeyboardButton::callback("🏛️ Contratación del Estado", "wiz:type:contratacion_estado")],
                vec![InlineKeyboardButton::callback("📰 BOE (RSS)", "wiz:type:boe_rss")],
                vec![InlineKeyboardButton::callback("🌏 CAIB Licitaciones", "wiz:type:caib_licitaciones")],
                vec![InlineKeyboardButton::callback("🖼️ Página web genérica (HTML)", "wiz:type:generic_html")],
            ]))
            .await?;
        }
        WizardStep::AskType => {
            // Este paso se maneja por callback, no debería llegar texto
            bot.send_message(chat_id, "Por favor selecciona una opción del menú.").await?;
        }
        WizardStep::AskKeywords => {
            let data = WizardData {
                keywords: if text.trim().is_empty() { None } else { Some(text.clone()) },
                ..wiz.data.clone()
            };
            wizard::update_wizard_data(user_id, data);
            wizard::set_wizard_step(user_id, WizardStep::AskSelector);
            bot.send_message(
                chat_id,
                "✅ Palabras clave guardadas.\n\nSi conoces un selector CSS específico, escríbelo. Si no, envía un punto (<code>.</code>) para omitir.",
            )
            .parse_mode(ParseMode::Html)
            .reply_markup(cancel_keyboard())
            .await?;
        }
        WizardStep::AskSelector => {
            let mut selector = if text.trim() == "." { None } else { Some(text.clone()) };
            let search_type = wiz.data.search_type.as_deref().unwrap_or("generic_html");

            // Auto-learn si no hay selector y es web generica
            if selector.is_none() && search_type == "generic_html" {
                if let Some(url) = wiz.data.url.as_deref() {
                    bot.send_message(chat_id, "🧠 Analizando la página para detectar el mejor selector CSS...").await?;
                    match auto_learn::learn_profile(&state.db, url).await {
                        Ok(profile) if profile.confidence >= 50 => {
                            selector = profile.item_selector.clone();
                            bot.send_message(
                                chat_id,
                                format!(
                                    "💡 <b>Selector detectado automáticamente</b>\n\n\
                                    Dominio: <code>{}</code>\n\
                                    Selector: <code>{}</code>\n\
                                    Confianza: {}%",
                                    profile.domain,
                                    profile.item_selector.as_deref().unwrap_or("-"),
                                    profile.confidence
                                ),
                            )
                            .parse_mode(ParseMode::Html)
                            .await?;
                        }
                        Ok(profile) => {
                            bot.send_message(
                                chat_id,
                                format!(
                                    "⚠️ No se pudo detectar un selector fiable (confianza: {}%). Continuamos sin selector.",
                                    profile.confidence
                                ),
                            )
                            .await?;
                        }
                        Err(e) => {
                            tracing::warn!("Error en auto-learn: {}", e);
                            bot.send_message(chat_id, "⚠️ No se pudo analizar la página automáticamente. Continuamos sin selector.").await?;
                        }
                    }
                }
            }

            let data = WizardData {
                css_selector: selector,
                ..wiz.data.clone()
            };
            wizard::update_wizard_data(user_id, data);
            wizard::set_wizard_step(user_id, WizardStep::AskFilters);

            bot.send_message(
                chat_id,
                "✅ Selector guardado.\n\nPaso 6/7: ¿Quieres añadir filtros avanzados?\n\n\
                <b>Formato:</b>\n\
                • <code>+Baleares,+Mallorca</code> (solo resultados que contengan ambas)\n\
                • <code>-Cáceres,-expirado</code> (excluir)\n\
                • <code>+Baleares,-Cáceres</code> (combinado)\n\n\
                Envía un punto (<code>.</code>) para omitir filtros.",
            )
            .parse_mode(ParseMode::Html)
            .reply_markup(cancel_keyboard())
            .await?;
        }
        WizardStep::AskFilters => {
            let data = WizardData {
                filters: if text.trim() == "." { None } else { Some(text.clone()) },
                ..wiz.data.clone()
            };
            wizard::update_wizard_data(user_id, data);
            wizard::set_wizard_step(user_id, WizardStep::AskNotifyMode);

            bot.send_message(
                chat_id,
                "✅ Filtros guardados.\n\nPaso 7/7: ¿Cómo quieres recibir las notificaciones?",
            )
            .reply_markup(InlineKeyboardMarkup::new(vec![
                vec![InlineKeyboardButton::callback("🚨 Inmediatas (alerta al detectar)", "wiz:notify:immediate")],
                vec![InlineKeyboardButton::callback("📊 Diarias (informe resumen)", "wiz:notify:daily")],
                vec![InlineKeyboardButton::callback("🔔 Ambas", "wiz:notify:both")],
            ]))
            .await?;
        }
        WizardStep::AskNotifyMode => {
            // Este paso se maneja por callback
            bot.send_message(chat_id, "Por favor selecciona una opción del menú.").await?;
        }
        WizardStep::Confirm => {
            bot.send_message(chat_id, "Usa los botones de arriba para confirmar o cancelar.").await?;
        }
    }

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

async fn handle_edit_text(
    bot: Bot,
    msg: Message,
    _state: Arc<BotState>,
    edit: wizard::EditState,
    text: String,
) -> anyhow::Result<()> {
    use crate::wizard::{self, EditStep};

    let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);
    let chat_id = msg.chat.id;

    match edit.step {
        EditStep::EditName => {
            let mut new_data = edit.data.clone();
            new_data.name = Some(text.clone());
            wizard::update_edit_data(user_id, new_data.clone());
            wizard::set_edit_step(user_id, EditStep::Confirm);
            bot.send_message(
                chat_id,
                format!("✅ Nombre actualizado a: <b>{}</b>\n\n¿Quieres modificar algo más?", text),
            )
            .parse_mode(ParseMode::Html)
            .reply_markup(edit_more_keyboard())
            .await?;
        }
        EditStep::EditUrl => {
            let mut new_data = edit.data.clone();
            new_data.url = Some(text.clone());
            wizard::update_edit_data(user_id, new_data.clone());
            wizard::set_edit_step(user_id, EditStep::Confirm);
            bot.send_message(
                chat_id,
                format!("✅ URL actualizada a: <b>{}</b>\n\n¿Quieres modificar algo más?", text),
            )
            .parse_mode(ParseMode::Html)
            .reply_markup(edit_more_keyboard())
            .await?;
        }
        EditStep::EditKeywords => {
            let mut new_data = edit.data.clone();
            new_data.keywords = if text.trim().is_empty() || text.trim() == "." { None } else { Some(text.clone()) };
            wizard::update_edit_data(user_id, new_data.clone());
            wizard::set_edit_step(user_id, EditStep::Confirm);
            bot.send_message(
                chat_id,
                format!("✅ Palabras clave actualizadas a: <b>{}</b>\n\n¿Quieres modificar algo más?", new_data.keywords.as_deref().unwrap_or("(ninguna)")),
            )
            .parse_mode(ParseMode::Html)
            .reply_markup(edit_more_keyboard())
            .await?;
        }
        EditStep::EditSelector => {
            let mut new_data = edit.data.clone();
            new_data.css_selector = if text.trim().is_empty() || text.trim() == "." { None } else { Some(text.clone()) };
            wizard::update_edit_data(user_id, new_data.clone());
            wizard::set_edit_step(user_id, EditStep::Confirm);
            bot.send_message(
                chat_id,
                format!("✅ Selector CSS actualizado a: <code>{}</code>\n\n¿Quieres modificar algo más?", new_data.css_selector.as_deref().unwrap_or("(ninguno)")),
            )
            .parse_mode(ParseMode::Html)
            .reply_markup(edit_more_keyboard())
            .await?;
        }
        EditStep::EditNotifyMode => {
            bot.send_message(chat_id, "Por favor selecciona una opción del menú de notificación.").await?;
        }
        EditStep::EditFilters => {
            let mut new_data = edit.data.clone();
            new_data.filters = if text.trim().is_empty() || text.trim() == "." { None } else { Some(text.clone()) };
            wizard::update_edit_data(user_id, new_data.clone());
            wizard::set_edit_step(user_id, EditStep::Confirm);
            let display = new_data.filters.as_deref()
                .map(|f| FilterConfig::parse(f).to_display_string())
                .unwrap_or_else(|| "(sin filtros)".to_string());
            bot.send_message(
                chat_id,
                format!("✅ Filtros actualizados: <b>{}</b>\n\n¿Quieres modificar algo más?", display),
            )
            .parse_mode(ParseMode::Html)
            .reply_markup(edit_more_keyboard())
            .await?;
        }
        EditStep::ChooseField | EditStep::Confirm => {
            bot.send_message(chat_id, "Usa los botones de arriba para seleccionar una acción.").await?;
        }
    }

    Ok(())
}

fn edit_more_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("✏️ Modificar otro campo", "editfield:menu"),
            InlineKeyboardButton::callback("✅ Guardar todo", "editsave"),
        ],
        vec![
            InlineKeyboardButton::callback("◀️ Atrás", "menu:busquedas"),
            InlineKeyboardButton::callback("🏠 Inicio", "menu:start"),
        ],
    ])
}
