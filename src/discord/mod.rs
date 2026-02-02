use serenity::async_trait;
use serenity::model::application::Interaction;
use serenity::model::gateway::Ready;
use serenity::model::id::GuildId;
use serenity::prelude::*;
use serenity::builder::{CreateInteractionResponse, CreateInteractionResponseMessage};
use tokio::sync::mpsc;

use crate::ServersConfig;

pub enum DiscordCommand {
    ListServers,
    SwitchServer(String),
    GetActiveServer,
    RemoveServer(String),
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
        serenity::builder::CreateCommand::new("servers")
            .description("List all configured Rust servers"),
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
        serenity::builder::CreateCommand::new("current")
            .description("Show currently connected Rust server"),
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
