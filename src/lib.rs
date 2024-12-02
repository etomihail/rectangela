use serde_json::json;
use tg_flows::{listen_to_update, Telegram, Update, UpdateKind, update_handler};
use store_flows::{get, set};
use flowsnet_platform_sdk::logger;
use llmservice_flows::{
    chat::{ChatOptions},
    LLMServiceFlows,
};
use std::env;

#[no_mangle]
#[tokio::main(flavor = "current_thread")]
pub async fn on_deploy() {
    let telegram_token = std::env::var("telegram_token").unwrap();
    listen_to_update(telegram_token).await;
}

#[update_handler]
async fn handler(update: Update) {
    logger::init();

    let allowed_user_id = std::env::var("allowed_user_id").unwrap_or_default();
    let telegram_token = std::env::var("telegram_token").unwrap();
    let placeholder_text = std::env::var("placeholder").unwrap_or("Typing ...".to_string());
    let system_prompt = std::env::var("system_prompt").unwrap_or("You are a helpful assistant answering questions on Telegram.".to_string());
    let help_mesg = std::env::var("help_mesg").unwrap_or("I am your assistant on Telegram. Ask me any question! To start a new conversation, type the /restart command.".to_string());
    let llm_api_endpoint = env::var("llm_api_endpoint").unwrap_or("https://llama.us.gaianet.network/v1".to_string());
    let llm_model_name = env::var("llm_model_name").unwrap_or("llama".to_string());
    let llm_ctx_size = env::var("llm_ctx_size").unwrap_or("16384".to_string()).parse::<u32>().unwrap_or(0);
    let llm_api_key = env::var("llm_api_key").unwrap_or("LLAMAEDGE".to_string());

    let tele = Telegram::new(telegram_token.to_string());

    if let UpdateKind::Message(msg) = update.kind {
        let chat_id = msg.chat.id.to_string();
        log::info!("Received message from {}", chat_id);

        // Restrict access to the allowed user ID
        if chat_id != allowed_user_id {
            log::warn!("Unauthorized access attempt from {}", chat_id);
            _ = tele.send_message(msg.chat.id, "You are not authorized to use this bot.");
            return;
        }

        let mut lf = LLMServiceFlows::new(&llm_api_endpoint);
        lf.set_api_key(&llm_api_key);

        let mut co = ChatOptions {
            model: Some(&llm_model_name),
            token_limit: llm_ctx_size,
            restart: false,
            system_prompt: Some(&system_prompt),
            ..Default::default()
        };

        let text = msg.text().unwrap_or("");
        if text.eq_ignore_ascii_case("/help") {
            _ = tele.send_message(msg.chat.id, &help_mesg);

        } else if text.eq_ignore_ascii_case("/start") {
            _ = tele.send_message(msg.chat.id, &help_mesg);
            set(&chat_id, json!(true), None);
            log::info!("Started conversation for {}", chat_id);

        } else if text.eq_ignore_ascii_case("/restart") {
            _ = tele.send_message(msg.chat.id, "Ok, I am starting a new conversation.");
            set(&chat_id, json!(true), None);
            log::info!("Restarted conversation for {}", chat_id);

        } else {
            let placeholder = tele
                .send_message(msg.chat.id, &placeholder_text)
                .expect("Error occurs when sending Message to Telegram");

            let restart = match get(&chat_id) {
                Some(v) => v.as_bool().unwrap_or_default(),
                None => false,
            };
            if restart {
                log::info!("Detected restart = true");
                set(&chat_id, json!(false), None);
                co.restart = true;
            }

            match lf.chat_completion(&chat_id, &text, &co).await {
                Ok(r) => {
                    _ = tele.edit_message_text(msg.chat.id, placeholder.id, r.choice);
                }
                Err(e) => {
                    _ = tele.edit_message_text(msg.chat.id, placeholder.id, "Sorry, an error has occurred. Please try again later!");
                    log::error!("LLM service returns error: {}", e);
                }
            }
        }
    }
}
