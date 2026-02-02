mod rustplus {
    include!(concat!(env!("OUT_DIR"), "/rustplus.rs"));
}

mod discord;

use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use futures_util::{StreamExt, SinkExt};
use serde::{Deserialize, Serialize};
use prost::Message as ProstMessage;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use rustplus::{AppRequest, AppMessage, AppEmpty, AppSendMessage, AppTime, AppInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServerConfig {
    name: String,
    server_ip: String,
    server_port: u16,
    player_id: String,
    player_token: String,
}

impl ServerConfig {
    fn get_player_token_i32(&self) -> Result<i32, Box<dyn std::error::Error>> {
        self.player_token.parse::<i32>()
            .map_err(|e| format!("Failed to parse player_token: {}", e).into())
    }

    fn get_player_id_u64(&self) -> Result<u64, Box<dyn std::error::Error>> {
        self.player_id.parse::<u64>()
            .map_err(|e| format!("Failed to parse player_id: {}", e).into())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ServersConfig {
    active_server: String,
    servers: std::collections::HashMap<String, ServerConfig>,
}

impl ServersConfig {
    fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = "servers.json";

        if !Path::new(config_path).exists() {
            return Err(format!(
                "Config file not found: {}\n\
                Please run: node capture_pairing.js\n\
                Then pair with your server in-game.",
                config_path
            ).into());
        }

        let config_data = fs::read_to_string(config_path)?;
        let config: ServersConfig = serde_json::from_str(&config_data)?;

        Ok(config)
    }

    fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_data = serde_json::to_string_pretty(self)?;
        fs::write("servers.json", config_data)?;
        Ok(())
    }

    fn get_active_server(&self) -> Result<ServerConfig, Box<dyn std::error::Error>> {
        self.servers
            .get(&self.active_server)
            .cloned()
            .ok_or_else(|| format!("Active server '{}' not found in configuration", self.active_server).into())
    }

    fn switch_server(&mut self, server_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        if !self.servers.contains_key(server_id) {
            return Err(format!("Server '{}' not found", server_id).into());
        }
        self.active_server = server_id.to_string();
        self.save()?;
        Ok(())
    }

    fn add_server(&mut self, server_id: String, config: ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
        self.servers.insert(server_id.clone(), config);
        if self.servers.len() == 1 {
            self.active_server = server_id;
        }
        self.save()?;
        Ok(())
    }

    fn remove_server(&mut self, server_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        if server_id == self.active_server {
            return Err("Cannot remove the currently active server. Switch to another server first.".into());
        }

        if !self.servers.contains_key(server_id) {
            return Err(format!("Server '{}' not found", server_id).into());
        }

        self.servers.remove(server_id);
        self.save()?;
        Ok(())
    }

    fn list_servers(&self) -> Vec<(String, ServerConfig)> {
        self.servers
            .iter()
            .map(|(id, config)| (id.clone(), config.clone()))
            .collect()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DiscordConfig {
    bot_token: String,
    application_id: u64,
    guild_id: u64,
}

impl DiscordConfig {
    fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = "discord_config.json";

        if !Path::new(config_path).exists() {
            return Err("discord_config.json not found. Please create it with your bot token and guild ID.".into());
        }

        let config_data = fs::read_to_string(config_path)?;
        let config: DiscordConfig = serde_json::from_str(&config_data)?;

        if config.bot_token == "YOUR_DISCORD_BOT_TOKEN_HERE" {
            return Err("Please set your Discord bot token in discord_config.json".into());
        }

        Ok(config)
    }
}

#[derive(Debug, Clone, Default)]
struct ServerState {
    time: Option<AppTime>,
    info: Option<AppInfo>,
}

impl ServerState {
    fn calculate_time_until_night(&self) -> Option<String> {
        let time = self.time.as_ref()?;
        let current = time.time;
        let sunset = time.sunset;
        let sunrise = time.sunrise;
        let day_length_minutes = time.day_length_minutes;

        // day_length_minutes appears to be the FULL 24-hour cycle time
        // Assume 15 min nights (standard), so daytime = total - 15
        const NIGHT_LENGTH_MINUTES: f32 = 15.0;
        let actual_day_minutes = day_length_minutes - NIGHT_LENGTH_MINUTES;

        let daytime_hours = sunset - sunrise;
        let nighttime_hours = 24.0 - daytime_hours;

        let is_daytime = current >= sunrise && current < sunset;

        let real_minutes_until = if is_daytime {
            // Currently day, calculate time until sunset
            let hours_until_sunset = sunset - current;
            hours_until_sunset * (actual_day_minutes / daytime_hours)
        } else {
            // Currently night, need to get through night + full day
            let hours_of_night_remaining = if current >= sunset {
                (24.0 - current) + sunrise
            } else {
                sunrise - current
            };
            let night_time = hours_of_night_remaining * (NIGHT_LENGTH_MINUTES / nighttime_hours);
            night_time + actual_day_minutes
        };

        let minutes = real_minutes_until.floor() as u32;
        let seconds = ((real_minutes_until - minutes as f32) * 60.0) as u32;

        Some(format!("{} minutes and {} seconds until night", minutes, seconds))
    }

    fn calculate_time_until_day(&self) -> Option<String> {
        let time = self.time.as_ref()?;
        let current = time.time;
        let sunrise = time.sunrise;
        let sunset = time.sunset;
        let day_length_minutes = time.day_length_minutes;

        // day_length_minutes appears to be the FULL 24-hour cycle time
        // Assume 15 min nights (standard), so daytime = total - 15
        const NIGHT_LENGTH_MINUTES: f32 = 15.0;
        let actual_day_minutes = day_length_minutes - NIGHT_LENGTH_MINUTES;

        let daytime_hours = sunset - sunrise;
        let nighttime_hours = 24.0 - daytime_hours;

        let is_daytime = current >= sunrise && current < sunset;

        let real_minutes_until = if is_daytime {
            // Currently day, need to get through rest of day + full night
            let hours_until_sunset = sunset - current;
            let day_time = hours_until_sunset * (actual_day_minutes / daytime_hours);
            day_time + NIGHT_LENGTH_MINUTES
        } else {
            // Currently night, calculate time until sunrise
            let hours_until_sunrise = if current >= sunset {
                (24.0 - current) + sunrise
            } else {
                sunrise - current
            };
            hours_until_sunrise * (NIGHT_LENGTH_MINUTES / nighttime_hours)
        };

        let minutes = real_minutes_until.floor() as u32;
        let seconds = ((real_minutes_until - minutes as f32) * 60.0) as u32;

        Some(format!("{} minutes and {} seconds until day", minutes, seconds))
    }

    fn get_current_time_info(&self) -> Option<String> {
        let time = self.time.as_ref()?;
        let current = time.time;

        let hours = current.floor() as u32;
        let minutes = ((current - hours as f32) * 60.0) as u32;

        let period = if current >= time.sunrise && current < time.sunset {
            "Day"
        } else {
            "Night"
        };

        Some(format!(
            "Current time: {:02}:{:02} ({})",
            hours, minutes, period
        ))
    }

    fn get_server_info(&self) -> Option<String> {
        let info = self.info.as_ref()?;
        Some(format!(
            "{} | {}/{} players | Map: {}m | Seed: {}",
            info.name,
            info.players,
            info.max_players,
            info.map_size,
            info.seed
        ))
    }
}

async fn start_pairing_listener() -> Option<tokio::process::Child> {
    use tokio::process::Command;

    let script_path = "capture_pairing.js";

    if !Path::new(script_path).exists() {
        eprintln!("Warning: {} not found. Automatic pairing disabled.", script_path);
        return None;
    }

    println!("Starting automatic pairing listener...");
    println!("Running: node {}", script_path);

    let current_dir = std::env::current_dir().unwrap_or_default();
    println!("Working directory: {}", current_dir.display());

    match Command::new("node")
        .arg(script_path)
        .current_dir(&current_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            println!("Process spawned with PID: {:?}", child.id());

            let stdout = child.stdout.take();
            let stderr = child.stderr.take();

            if let Some(stdout) = stdout {
                tokio::spawn(async move {
                    use tokio::io::{AsyncBufReadExt, BufReader};
                    let reader = BufReader::new(stdout);
                    let mut lines = reader.lines();

                    while let Ok(Some(line)) = lines.next_line().await {
                        println!("[Pairing] {}", line);
                    }
                });
            }

            if let Some(stderr) = stderr {
                tokio::spawn(async move {
                    use tokio::io::{AsyncBufReadExt, BufReader};
                    let reader = BufReader::new(stderr);
                    let mut lines = reader.lines();

                    while let Ok(Some(line)) = lines.next_line().await {
                        eprintln!("[Pairing] {}", line);
                    }
                });
            }

            println!("Pairing listener started successfully\n");
            Some(child)
        }
        Err(e) => {
            eprintln!("Warning: Failed to start pairing listener: {}", e);
            eprintln!("Make sure Node.js is installed. Automatic pairing disabled.\n");
            None
        }
    }
}

#[tokio::main]
async fn main() {
    println!("=== RustServant ===");
    println!("Starting up...\n");

    let mut pairing_script = start_pairing_listener().await;

    let discord_config = match DiscordConfig::load() {
        Ok(cfg) => {
            println!("Starting Discord bot...");
            Some(cfg)
        }
        Err(e) => {
            eprintln!("Warning: Discord bot disabled: {}", e);
            eprintln!("Continuing with Rust+ only...\n");
            None
        }
    };

    let mut discord_command_rx = if let Some(discord_cfg) = discord_config {
        match discord::start_bot(discord_cfg.bot_token, discord_cfg.application_id, discord_cfg.guild_id).await {
            Ok((mut client, command_rx)) => {
                tokio::spawn(async move {
                    if let Err(e) = client.start().await {
                        eprintln!("Discord bot error: {}", e);
                    }
                });
                println!("Discord bot started successfully\n");
                Some(command_rx)
            }
            Err(e) => {
                eprintln!("Failed to start Discord bot: {}", e);
                eprintln!("Continuing with Rust+ only...\n");
                None
            }
        }
    } else {
        None
    };

    loop {
        let servers_config = match ServersConfig::load() {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Failed to load configuration:");
                eprintln!("  {}\n", e);
                eprintln!("To get started:");
                eprintln!("  1. Run: node capture_pairing.js");
                eprintln!("  2. In Rust game: ESC -> Rust+ -> Pair with Server");
                eprintln!("  3. Run this program again");
                return;
            }
        };

        let config = match servers_config.get_active_server() {
            Ok(cfg) => {
                println!("Loaded server configuration");
                println!("  Active Server: {}", servers_config.active_server);
                println!("  Name: {}", cfg.name);
                println!("  Address: {}:{}", cfg.server_ip, cfg.server_port);
                println!("  Player: {}", cfg.player_id);
                println!();
                cfg
            }
            Err(e) => {
                eprintln!("Failed to load active server:");
                eprintln!("  {}", e);
                return;
            }
        };

        let player_id = match config.get_player_id_u64() {
            Ok(id) => id,
            Err(e) => {
                eprintln!("{}", e);
                return;
            }
        };

        let player_token = match config.get_player_token_i32() {
            Ok(token) => token,
            Err(e) => {
                eprintln!("{}", e);
                return;
            }
        };

        let should_reconnect = run_rustplus_connection(
            config,
            player_id,
            player_token,
            &mut discord_command_rx,
        ).await;

        if !should_reconnect {
            break;
        }

        println!("\nReconnecting to new server...\n");
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    if let Some(mut child) = pairing_script {
        println!("\nStopping pairing listener...");
        let _ = child.kill().await;
    }

    println!("Shutdown complete.");
}

async fn run_rustplus_connection(
    config: ServerConfig,
    player_id: u64,
    player_token: i32,
    discord_command_rx: &mut Option<tokio::sync::mpsc::UnboundedReceiver<discord::DiscordCommand>>,
) -> bool {

    let url = format!("ws://{}:{}/", config.server_ip, config.server_port);
    println!("Attempting to connect to: {}", url);

    match connect_async(&url).await {
        Ok((ws_stream, _)) => {
            println!("Connected successfully!");

            let (write, read) = ws_stream.split();
            let write = Arc::new(tokio::sync::Mutex::new(write));
            let state = Arc::new(RwLock::new(ServerState::default()));
            let write_clone = Arc::clone(&write);
            tokio::spawn(async move {
                let mut seq = 1000u32;
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

                loop {
                    interval.tick().await;

                    seq += 1;
                    let time_request = AppRequest {
                        seq,
                        player_id,
                        player_token,
                        entity_id: 0,
                        get_time: Some(AppEmpty {}),
                        ..Default::default()
                    };

                    let mut buf = Vec::new();
                    if time_request.encode(&mut buf).is_ok() {
                        let mut write_guard = write_clone.lock().await;
                        let _ = write_guard.send(Message::Binary(buf)).await;
                    }

                    seq += 1;
                    let info_request = AppRequest {
                        seq,
                        player_id,
                        player_token,
                        entity_id: 0,
                        get_info: Some(AppEmpty {}),
                        ..Default::default()
                    };

                    let mut buf = Vec::new();
                    if info_request.encode(&mut buf).is_ok() {
                        let mut write_guard = write_clone.lock().await;
                        let _ = write_guard.send(Message::Binary(buf)).await;
                    }
                }
            });

            {
                let mut write_guard = write.lock().await;

                let time_request = AppRequest {
                    seq: 1,
                    player_id,
                    player_token,
                    entity_id: 0,
                    get_time: Some(AppEmpty {}),
                    ..Default::default()
                };
                let mut buf = Vec::new();
                time_request.encode(&mut buf).unwrap();
                let _ = write_guard.send(Message::Binary(buf)).await;

                let info_request = AppRequest {
                    seq: 2,
                    player_id,
                    player_token,
                    entity_id: 0,
                    get_info: Some(AppEmpty {}),
                    ..Default::default()
                };
                let mut buf = Vec::new();
                info_request.encode(&mut buf).unwrap();
                let _ = write_guard.send(Message::Binary(buf)).await;
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

            println!("Monitoring team chat for commands...");
            println!("Commands: !time, !night, !day, !info");
            println!("Press Ctrl+C to exit.\n");

            let mut read = read;
            let mut seq_counter = 100u32;

            loop {
                tokio::select! {
                    Some(discord_cmd) = async {
                        match discord_command_rx.as_mut() {
                            Some(rx) => rx.recv().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        match discord_cmd {
                            discord::DiscordCommand::SwitchServer(_) => {
                                println!("Received server switch command from Discord");
                                return true;
                            }
                            _ => {}
                        }
                    }

                    Some(msg) = read.next() => {
                        match msg {
                    Ok(Message::Binary(data)) => {
                        match AppMessage::decode(&data[..]) {
                            Ok(app_msg) => {
                                if let Some(response) = app_msg.response {
                                    let mut state_guard = state.write().await;

                                    if let Some(time) = response.time {
                                        state_guard.time = Some(time);
                                        println!("Updated server time");
                                    }

                                    if let Some(info) = response.info {
                                        state_guard.info = Some(info);
                                        println!("Updated server info");
                                    }

                                    if let Some(error) = response.error {
                                        eprintln!("API Error: {}", error.error);

                                        if error.error.contains("not paired") ||
                                           error.error.contains("auth") ||
                                           error.error.contains("token") ||
                                           error.error.contains("unauthorized") {
                                            eprintln!("\n! Token appears to be invalid or expired");
                                            eprintln!("! To re-pair this server:");
                                            eprintln!("!   1. Run: node capture_pairing.js");
                                            eprintln!("!   2. In-game: ESC -> Rust+ -> Pair with Server");
                                            eprintln!("!   3. Restart RustServant");
                                            eprintln!("\nShutting down due to authentication failure...");
                                            return false;
                                        }
                                    }
                                }

                                if let Some(broadcast) = app_msg.broadcast {
                                    if let Some(new_team_msg) = broadcast.team_message {
                                        if let Some(team_message) = new_team_msg.message {
                                            let name = &team_message.name;
                                            let message_text = &team_message.message;

                                            println!("[Team] {}: {}", name, message_text);

                                            if message_text.starts_with('!') {
                                                let command = message_text.trim().to_lowercase();
                                                let state_guard = state.read().await;

                                                let response_text = match command.as_str() {
                                                    "!time" => state_guard.get_current_time_info(),
                                                    "!night" => state_guard.calculate_time_until_night(),
                                                    "!day" => state_guard.calculate_time_until_day(),
                                                    "!info" => state_guard.get_server_info(),
                                                    _ => Some(format!("Unknown command. Try: !time, !night, !day, !info")),
                                                };

                                                drop(state_guard);

                                                if let Some(response) = response_text {
                                                    seq_counter += 1;
                                                    let send_request = AppRequest {
                                                        seq: seq_counter,
                                                        player_id,
                                                        player_token,
                                                        entity_id: 0,
                                                        send_team_message: Some(AppSendMessage {
                                                            message: response.clone(),
                                                        }),
                                                        ..Default::default()
                                                    };

                                                    let mut buf = Vec::new();
                                                    if send_request.encode(&mut buf).is_ok() {
                                                        let mut write_guard = write.lock().await;
                                                        if let Err(e) = write_guard.send(Message::Binary(buf)).await {
                                                            eprintln!("Failed to send: {}", e);
                                                        } else {
                                                            println!("[Sent] {}", response);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to decode: {}", e);
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        println!("Connection closed");
                        return false;
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        return false;
                    }
                    _ => {}
                }
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to connect: {}", e);
            eprintln!("\nMake sure:");
            eprintln!("  1. The Rust server is running");
            eprintln!("  2. Rust+ is enabled on the server");
            eprintln!("  3. You're paired with the correct server");
            eprintln!("  4. Your pairing hasn't expired");
            eprintln!("\nTo re-pair:");
            eprintln!("  1. Run: node capture_pairing.js");
            eprintln!("  2. In-game: ESC -> Rust+ -> Pair with Server");
            return false;
        }
    }
}
