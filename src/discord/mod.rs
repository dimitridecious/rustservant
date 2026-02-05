use serenity::async_trait;
use serenity::model::application::Interaction;
use serenity::model::gateway::Ready;
use serenity::model::id::GuildId;
use serenity::prelude::*;
use serenity::builder::{CreateInteractionResponse, CreateInteractionResponseMessage};
use tokio::sync::mpsc;
use std::path::Path;

use crate::ServersConfig;

pub enum DiscordCommand {
    ListServers,
    SwitchServer(String),
    GetActiveServer,
    RemoveServer(String),
    StartPairing,
    Help,
}

pub struct Handler {
    pub command_tx: mpsc::UnboundedSender<DiscordCommand>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            let response_content = match command.data.name.as_str() {
                "servers" => {
                    let _ = self.command_tx.send(DiscordCommand::ListServers);

                    if let Ok(config) = ServersConfig::load() {
                        let servers = config.list_servers();
                        if servers.is_empty() {
                            "No servers configured. Pair with a server using the pairing script.".to_string()
                        } else {
                            let mut msg = format!("**Configured Servers ({}):**\n\n", servers.len());
                            for (id, server) in servers {
                                let active = if id == config.active_server {
                                    " [ACTIVE]"
                                } else {
                                    ""
                                };
                                msg.push_str(&format!(
                                    "**{}**{}\n  {} ({}:{})\n\n",
                                    id, active, server.name, server.server_ip, server.server_port
                                ));
                            }
                            msg
                        }
                    } else {
                        "Failed to load server configuration.".to_string()
                    }
                }
                "switch" => {
                    use serenity::model::application::CommandDataOptionValue;

                    let server_id = command.data.options.first()
                        .and_then(|opt| {
                            if let CommandDataOptionValue::String(s) = &opt.value {
                                Some(s.as_str())
                            } else {
                                None
                            }
                        })
                        .unwrap_or("");

                    if server_id.is_empty() {
                        "Please provide a server ID.".to_string()
                    } else {
                        let _ = self.command_tx.send(DiscordCommand::SwitchServer(server_id.to_string()));

                        match ServersConfig::load() {
                            Ok(mut config) => {
                                match config.switch_server(server_id) {
                                    Ok(_) => {
                                        format!("Switching to server: **{}**\n\nRustServant will reconnect...", server_id)
                                    }
                                    Err(e) => format!("Failed to switch: {}", e),
                                }
                            }
                            Err(e) => format!("Failed to load configuration: {}", e),
                        }
                    }
                }
                "current" => {
                    let _ = self.command_tx.send(DiscordCommand::GetActiveServer);

                    match ServersConfig::load() {
                        Ok(config) => {
                            match config.get_active_server() {
                                Ok(server) => {
                                    format!(
                                        "**Currently Connected:**\n\n**{}** ({})\n{}:{}\n\nPlayer: {}",
                                        config.active_server,
                                        server.name,
                                        server.server_ip,
                                        server.server_port,
                                        server.player_id
                                    )
                                }
                                Err(e) => format!("Failed to get active server: {}", e),
                            }
                        }
                        Err(e) => format!("Failed to load configuration: {}", e),
                    }
                }
                "remove" => {
                    use serenity::model::application::CommandDataOptionValue;

                    let server_id = command.data.options.first()
                        .and_then(|opt| {
                            if let CommandDataOptionValue::String(s) = &opt.value {
                                Some(s.as_str())
                            } else {
                                None
                            }
                        })
                        .unwrap_or("");

                    if server_id.is_empty() {
                        "Please provide a server ID.".to_string()
                    } else {
                        let _ = self.command_tx.send(DiscordCommand::RemoveServer(server_id.to_string()));

                        match ServersConfig::load() {
                            Ok(mut config) => {
                                match config.remove_server(server_id) {
                                    Ok(_) => {
                                        format!("Removed server: **{}**", server_id)
                                    }
                                    Err(e) => format!("Failed to remove: {}", e),
                                }
                            }
                            Err(e) => format!("Failed to load configuration: {}", e),
                        }
                    }
                }
                "help" => {
                    let _ = self.command_tx.send(DiscordCommand::Help);
                    "**RustServant Commands**\n\n\
                    **In-Game (Team Chat):**\n\
                    • `!help` - Show available commands\n\
                    • `!time` - Current time and next day/night cycle\n\
                    • `!night` - Time until nightfall\n\
                    • `!day` - Time until daytime\n\
                    • `!pop` - Player count and queue\n\
                    • `!info` - Server name\n\
                    • `!showevents` - List active events with locations\n\n\
                    **Debug Commands (Terminal Output Only):**\n\
                    • `!getmarkers` - Marker summary counts\n\
                    • `!debugmarkers` - Detailed marker information\n\
                    • `!mapdebug` - Map metadata and dimensions\n\
                    • `!testmonuments` - Monument grid coordinate testing\n\n\
                    **Discord Commands:**\n\
                    • `/help` - Show this help message\n\
                    • `/pair` - Pair with a new server (60s listener)\n\
                    • `/servers` - List all configured servers\n\
                    • `/current` - Show currently connected server\n\
                    • `/switch <server>` - Switch to different server\n\
                    • `/remove <server>` - Remove server from config".to_string()
                }
                "pair" => {
                    let _ = self.command_tx.send(DiscordCommand::StartPairing);

                    // Send initial response
                    let initial_response = "**Pairing Mode Activated**\n\n\
                    Listening for 60 seconds...\n\n\
                    **Steps:**\n\
                    1. Open Rust game\n\
                    2. Press ESC\n\
                    3. Go to Rust+ menu\n\
                    4. Click \"Pair with Server\"\n\n\
                    Waiting for pairing notification...";

                    // Spawn pairing task
                    let http = ctx.http.clone();
                    let token = command.token.clone();

                    tokio::spawn(async move {
                        use tokio::process::Command;
                        use tokio::io::{AsyncBufReadExt, BufReader};

                        let script_path = "capture_pairing.js";

                        if !Path::new(script_path).exists() {
                            let _ = http.create_followup_message(&token, &serenity::all::CreateInteractionResponseFollowup::new()
                                .content("❌ Error: capture_pairing.js not found"), vec![]).await;
                            return;
                        }

                        let mut child = match Command::new("node")
                            .arg(script_path)
                            .arg("--timeout=60")
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .spawn()
                        {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = http.create_followup_message(&token, &serenity::all::CreateInteractionResponseFollowup::new()
                                    .content(format!("❌ Failed to start pairing: {}", e)), vec![]).await;
                                return;
                            }
                        };

                        let stdout = match child.stdout.take() {
                            Some(s) => s,
                            None => return,
                        };

                        let reader = BufReader::new(stdout);
                        let mut lines = reader.lines();

                        let mut server_name = String::new();
                        let mut server_id = String::new();
                        let mut server_address = String::new();
                        let mut is_new = false;

                        while let Ok(Some(line)) = lines.next_line().await {
                            if line == "PAIRING_LISTENING" {
                                println!("[Pairing] Listening for 60 seconds...");
                            } else if line == "PAIRING_SUCCESS" {
                                while let Ok(Some(info_line)) = lines.next_line().await {
                                    if info_line.starts_with("SERVER_NAME:") {
                                        server_name = info_line.replace("SERVER_NAME:", "");
                                    } else if info_line.starts_with("SERVER_ID:") {
                                        server_id = info_line.replace("SERVER_ID:", "");
                                    } else if info_line.starts_with("SERVER_ADDRESS:") {
                                        server_address = info_line.replace("SERVER_ADDRESS:", "");
                                    } else if info_line.starts_with("IS_NEW:") {
                                        is_new = info_line.replace("IS_NEW:", "") == "true";
                                    }
                                }
                                break;
                            } else if line == "PAIRING_TIMEOUT" {
                                println!("[Pairing] Timed out after 60 seconds");
                                let _ = child.kill().await;
                                let _ = http.create_followup_message(&token, &serenity::all::CreateInteractionResponseFollowup::new()
                                    .content("⏱️ Pairing timed out after 60 seconds. Please try again."), vec![]).await;
                                return;
                            } else if line.starts_with("PAIRING_ERROR:") {
                                let error = line.replace("PAIRING_ERROR:", "");
                                println!("[Pairing] Error: {}", error);
                                let _ = child.kill().await;
                                let _ = http.create_followup_message(&token, &serenity::all::CreateInteractionResponseFollowup::new()
                                    .content(format!("❌ Pairing error: {}", error)), vec![]).await;
                                return;
                            }
                        }

                        let _ = child.wait().await;

                        if !server_name.is_empty() {
                            let status_text = if is_new { "Added new server" } else { "Updated existing server" };
                            println!("[Pairing] {} successfully: {} ({})", status_text, server_name, server_address);

                            let message = format!("✅ {} **{}**\n\nServer ID: `{}`\nAddress: `{}`\n\n_Use `/switch {}` to connect to this server_",
                                status_text, server_name, server_id, server_address, server_id);
                            let _ = http.create_followup_message(&token, &serenity::all::CreateInteractionResponseFollowup::new()
                                .content(message), vec![]).await;
                        } else {
                            println!("[Pairing] Failed - no server detected");
                            let _ = http.create_followup_message(&token, &serenity::all::CreateInteractionResponseFollowup::new()
                                .content("❌ Pairing failed. No server detected."), vec![]).await;
                        }
                    });

                    initial_response.to_string()
                }
                _ => "Unknown command".to_string(),
            };

            let data = CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(response_content)
            );

            if let Err(e) = command.create_response(&ctx.http, data).await {
                eprintln!("Cannot respond to slash command: {}", e);
            }
        }
    }

    async fn ready(&self, _ctx: Context, ready: Ready) {
        println!("Discord bot connected as: {}", ready.user.name);
    }
}

pub async fn start_bot(
    token: String,
    application_id: u64,
    guild_id: u64,
) -> Result<(Client, mpsc::UnboundedReceiver<DiscordCommand>), Box<dyn std::error::Error>> {
    let (command_tx, command_rx) = mpsc::unbounded_channel();

    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    let client = Client::builder(&token, intents)
        .event_handler(Handler { command_tx })
        .application_id(application_id.into())
        .await?;

    let guild_id = GuildId::new(guild_id);

    let commands = vec![
        serenity::builder::CreateCommand::new("help")
            .description("Show all available commands"),
        serenity::builder::CreateCommand::new("pair")
            .description("Pair with a new Rust server (listens for 60 seconds)"),
        serenity::builder::CreateCommand::new("servers")
            .description("List all configured Rust servers"),
        serenity::builder::CreateCommand::new("current")
            .description("Show currently connected Rust server"),
        serenity::builder::CreateCommand::new("switch")
            .description("Switch to a different Rust server")
            .add_option(
                serenity::builder::CreateCommandOption::new(
                    serenity::model::application::CommandOptionType::String,
                    "server_id",
                    "The server ID to switch to"
                )
                .required(true)
            ),
        serenity::builder::CreateCommand::new("remove")
            .description("Remove a Rust server from configuration")
            .add_option(
                serenity::builder::CreateCommandOption::new(
                    serenity::model::application::CommandOptionType::String,
                    "server_id",
                    "The server ID to remove"
                )
                .required(true)
            ),
    ];

    guild_id.set_commands(&client.http, commands).await?;

    Ok((client, command_rx))
}
