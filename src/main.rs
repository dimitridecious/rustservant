#[allow(dead_code)]
mod rustplus {
    include!(concat!(env!("OUT_DIR"), "/rustplus.rs"));
}

mod discord;
mod helpers;
mod items;

use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use futures_util::{StreamExt, SinkExt};
use serde::{Deserialize, Serialize};
use prost::Message as ProstMessage;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use rustplus::{AppRequest, AppMessage, AppEmpty, AppSendMessage, AppTime, AppInfo, AppMapMarkers, AppMarkerType, AppMap};

enum ReconnectAction {
    SwitchServer,  // User requested server switch
    Retry,         // Connection lost, should retry
    Stop,          // Fatal error, stop completely
}

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
            return Err("No servers configured yet".into());
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

#[derive(Debug, Clone)]
struct ServerState {
    time: Option<AppTime>,
    info: Option<AppInfo>,
    markers: Option<AppMapMarkers>,
    map: Option<AppMap>,
}

impl Default for ServerState {
    fn default() -> Self {
        Self {
            time: None,
            info: None,
            markers: None,
            map: None,
        }
    }
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
        let sunrise = time.sunrise;
        let sunset = time.sunset;
        let day_length_minutes = time.day_length_minutes;

        let hours = current.floor() as u32;
        let minutes = ((current - hours as f32) * 60.0) as u32;

        let is_daytime = current >= sunrise && current < sunset;

        // Calculate time until next cycle
        const NIGHT_LENGTH_MINUTES: f32 = 15.0;
        let actual_day_minutes = day_length_minutes - NIGHT_LENGTH_MINUTES;
        let daytime_hours = sunset - sunrise;
        let nighttime_hours = 24.0 - daytime_hours;

        let real_minutes_until = if is_daytime {
            // Time until night
            let hours_until_sunset = sunset - current;
            hours_until_sunset * (actual_day_minutes / daytime_hours)
        } else {
            // Time until day
            let hours_until_sunrise = if current >= sunset {
                (24.0 - current) + sunrise
            } else {
                sunrise - current
            };
            hours_until_sunrise * (NIGHT_LENGTH_MINUTES / nighttime_hours)
        };

        let mins = real_minutes_until.floor() as u32;
        let secs = ((real_minutes_until - mins as f32) * 60.0) as u32;

        let next_cycle = if is_daytime { "Night" } else { "Day" };

        // Format: "14:23 (Day) - Night in 42m 34s" = ~30 chars
        Some(format!(
            "{:02}:{:02} ({}) - {} in {}m {}s",
            hours, minutes,
            if is_daytime { "Day" } else { "Night" },
            next_cycle, mins, secs
        ))
    }

    fn get_server_info(&self) -> Option<String> {
        let info = self.info.as_ref()?;
        Some(format!(
            "{}",
            info.name
        ))
    }

    fn get_population_info(&self) -> Option<String> {
        let info = self.info.as_ref()?;

        let queue_info = if info.queued_players > 0 {
            format!(" ({} in queue)", info.queued_players)
        } else {
            String::new()
        };

        Some(format!(
            "Players: {}/{}{}",
            info.players,
            info.max_players,
            queue_info
        ))
    }
}

#[tokio::main]
async fn main() {
    println!("=== RustServant ===\n");

    let discord_config = match DiscordConfig::load() {
        Ok(cfg) => {
            println!("Discord bot enabled");
            Some(cfg)
        }
        Err(e) => {
            eprintln!("Discord bot disabled: {}", e);
            eprintln!("Continuing with Rust+ only\n");
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
                Some(command_rx)
            }
            Err(e) => {
                eprintln!("Failed to start Discord bot: {}", e);
                eprintln!("Continuing with Rust+ only\n");
                None
            }
        }
    } else {
        None
    };

    let mut reconnect_attempts = 0;
    let max_reconnect_attempts = 5;

    loop {
        let servers_config = match ServersConfig::load() {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Configuration error: {}", e);
                eprintln!("Use Discord command /pair to add a server\n");
                return;
            }
        };

        let config = match servers_config.get_active_server() {
            Ok(cfg) => {
                if reconnect_attempts == 0 {
                    println!("Connecting to: {} ({}:{})\n", cfg.name, cfg.server_ip, cfg.server_port);
                } else {
                    println!("Reconnecting to: {} (attempt {}/{})\n", cfg.name, reconnect_attempts + 1, max_reconnect_attempts);
                }
                cfg
            }
            Err(e) => {
                eprintln!("Failed to load active server: {}", e);
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

        let reconnect_result = run_rustplus_connection(
            config,
            player_id,
            player_token,
            &mut discord_command_rx,
        ).await;

        match reconnect_result {
            ReconnectAction::SwitchServer => {
                println!("\nSwitching servers...\n");
                reconnect_attempts = 0;
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
            ReconnectAction::Retry => {
                reconnect_attempts += 1;
                if reconnect_attempts >= max_reconnect_attempts {
                    eprintln!("\nMax reconnection attempts ({}) reached. Giving up.", max_reconnect_attempts);
                    eprintln!("Check your connection or use /pair to re-authenticate\n");
                    break;
                }

                // Exponential backoff: 2s, 4s, 8s, 16s, 32s
                let delay = 2u64.pow(reconnect_attempts as u32);
                println!("Retrying in {} seconds...\n", delay);
                tokio::time::sleep(tokio::time::Duration::from_secs(delay)).await;
            }
            ReconnectAction::Stop => {
                break;
            }
        }
    }

    println!("Shutdown complete.");
}

async fn run_rustplus_connection(
    config: ServerConfig,
    player_id: u64,
    player_token: i32,
    discord_command_rx: &mut Option<tokio::sync::mpsc::UnboundedReceiver<discord::DiscordCommand>>,
) -> ReconnectAction {

    let url = format!("ws://{}:{}/", config.server_ip, config.server_port);

    match connect_async(&url).await {
        Ok((ws_stream, _)) => {
            println!("Connected to Rust+ API\n");

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

                let map_request = AppRequest {
                    seq: 3,
                    player_id,
                    player_token,
                    entity_id: 0,
                    get_map: Some(AppEmpty {}),
                    ..Default::default()
                };
                let mut buf = Vec::new();
                map_request.encode(&mut buf).unwrap();
                let _ = write_guard.send(Message::Binary(buf)).await;
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

            println!("Ready! Type !help in team chat for commands\n");

            let mut read = read;
            let mut seq_counter = 100u32;
            let last_markers_request_seq: Arc<RwLock<Option<(u32, bool, bool)>>> = Arc::new(RwLock::new(None)); // (seq, is_show_events, is_debug)

            loop {
                tokio::select! {
                    Some(discord_cmd) = async {
                        match discord_command_rx.as_mut() {
                            Some(rx) => rx.recv().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        match discord_cmd {
                            discord::DiscordCommand::SwitchServer(server_id) => {
                                println!("Switching to server: {}", server_id);
                                return ReconnectAction::SwitchServer;
                            }
                            discord::DiscordCommand::RemoveServer(server_id) => {
                                println!("Server removed: {}", server_id);
                                // Discord already handled the removal, no action needed here
                            }
                            discord::DiscordCommand::StartPairing => {
                                // Pairing is handled in Discord interaction response
                                // The actual process runs independently
                            }
                            discord::DiscordCommand::Help => {
                                // Help is handled in Discord interaction response
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
                                    }

                                    if let Some(info) = response.info {
                                        state_guard.info = Some(info);
                                    }

                                    if let Some(map) = response.map {
                                        let monument_count = map.monuments.len();
                                        state_guard.map = Some(map);
                                        println!("Map data loaded ({} monuments)", monument_count);
                                    }

                                    if let Some(markers) = response.map_markers {
                                        // Check if this was from our test command
                                        let request_info = {
                                            let last_seq = last_markers_request_seq.read().await;
                                            last_seq.and_then(|(seq, is_show, is_debug)| {
                                                if seq == response.seq {
                                                    Some((is_show, is_debug))
                                                } else {
                                                    None
                                                }
                                            })
                                        };

                                        if let Some((is_show_events, is_debug)) = request_info {
                                            let result = if is_debug {
                                                let non_vending: Vec<_> = markers.markers.iter()
                                                    .filter(|m| m.r#type() != AppMarkerType::VendingMachine)
                                                    .collect();

                                                let mut debug_info = format!("Marker Debug ({} non-vending):\n\n", non_vending.len());

                                                for (i, &marker) in non_vending.iter().enumerate().take(15) {
                                                    debug_info.push_str(&format!("{}. Type: {:?}\n", i + 1, marker.r#type()));
                                                    debug_info.push_str(&format!("   Name: '{}'\n", marker.name));
                                                    debug_info.push_str(&format!("   Coords: ({:.1}, {:.1})\n", marker.x, marker.y));
                                                    debug_info.push_str(&format!("   Radius: {:.1}\n", marker.radius));
                                                    debug_info.push_str(&format!("   ID: {}\n\n", marker.id));
                                                }

                                                if non_vending.len() > 15 {
                                                    debug_info.push_str(&format!("...and {} more", non_vending.len() - 15));
                                                }

                                                debug_info
                                            } else if is_show_events {
                                                // Show detailed event list with locations
                                                let map_size = state_guard.info.as_ref().map(|i| i.map_size).unwrap_or(4000);
                                                let mut events = Vec::new();

                                                for marker in &markers.markers {
                                                    let event_name = match marker.r#type() {
                                                        AppMarkerType::PatrolHelicopter => Some("Patrol Heli"),
                                                        AppMarkerType::CargoShip => Some("Cargo Ship"),
                                                        AppMarkerType::Crate => Some("Locked Crate"),
                                                        AppMarkerType::Ch47 => Some("Chinook"),
                                                        AppMarkerType::TravelingVendor => Some("Traveling Vendor"),
                                                        AppMarkerType::Explosion => Some("Explosion/Crash Site"),
                                                        _ => None,
                                                    };

                                                    if let Some(name) = event_name {
                                                        // Check if coordinates are within map bounds
                                                        let is_valid = marker.x >= 0.0
                                                            && marker.x <= map_size as f32
                                                            && marker.y >= 0.0
                                                            && marker.y <= map_size as f32;

                                                        let location_str = if is_valid {
                                                            let grid = helpers::coords_to_grid(marker.x, marker.y, map_size);
                                                            let region = helpers::get_map_region(marker.x, marker.y, map_size);
                                                            format!("{} at {} ({}) - coords: ({:.0}, {:.0})",
                                                                name, grid, region, marker.x, marker.y)
                                                        } else {
                                                            // Determine general direction for out-of-bounds
                                                            let x_dir = if marker.x < 0.0 {
                                                                "left"
                                                            } else if marker.x > map_size as f32 {
                                                                "right"
                                                            } else {
                                                                ""
                                                            };

                                                            let y_dir = if marker.y < 0.0 {
                                                                "bottom"
                                                            } else if marker.y > map_size as f32 {
                                                                "top"
                                                            } else {
                                                                ""
                                                            };

                                                            let direction = match (y_dir, x_dir) {
                                                                ("", "") => "outside grids".to_string(),
                                                                ("", x) => format!("outside grids - {}", x),
                                                                (y, "") => format!("outside grids - {}", y),
                                                                (y, x) => format!("outside grids - {} {}", y, x),
                                                            };

                                                            format!("{} ({})", name, direction)
                                                        };

                                                        events.push(location_str);
                                                    }
                                                }

                                                if events.is_empty() {
                                                    "No active events right now".to_string()
                                                } else {
                                                    format!("Active Events ({}):\n{}", events.len(), events.join("\n"))
                                                }
                                            } else {
                                                // Show summary counts (old !getmarkers behavior)
                                                let marker_count = markers.markers.len();
                                                let mut result = format!("Map Markers (total: {}):\n", marker_count);

                                                let mut heli_count = 0;
                                                let mut cargo_count = 0;
                                                let mut crate_count = 0;
                                                let mut chinook_count = 0;
                                                let mut vendor_count = 0;
                                                let mut vending_count = 0;
                                                let mut other_count = 0;

                                                for marker in &markers.markers {
                                                    match marker.r#type() {
                                                        AppMarkerType::PatrolHelicopter => heli_count += 1,
                                                        AppMarkerType::CargoShip => cargo_count += 1,
                                                        AppMarkerType::Crate => crate_count += 1,
                                                        AppMarkerType::Ch47 => chinook_count += 1,
                                                        AppMarkerType::TravelingVendor => vendor_count += 1,
                                                        AppMarkerType::VendingMachine => vending_count += 1,
                                                        _ => other_count += 1,
                                                    }
                                                }

                                                result.push_str(&format!("Patrol Heli: {}\n", heli_count));
                                                result.push_str(&format!("Cargo Ship: {}\n", cargo_count));
                                                result.push_str(&format!("Locked Crates: {}\n", crate_count));
                                                result.push_str(&format!("Chinook: {}\n", chinook_count));
                                                result.push_str(&format!("Traveling Vendor: {}\n", vendor_count));
                                                result.push_str(&format!("Vending Machines: {}\n", vending_count));
                                                result.push_str(&format!("Other: {}", other_count));
                                                result
                                            };

                                            // For debug commands, only print to terminal
                                            if is_debug || (!is_show_events && !is_debug) {
                                                // !debugmarkers or !getmarkers - terminal only
                                                println!("[Debug Output]\n{}", result);
                                            } else {
                                                // !showevents - send to team chat
                                                seq_counter += 1;
                                                let send_request = AppRequest {
                                                    seq: seq_counter,
                                                    player_id,
                                                    player_token,
                                                    entity_id: 0,
                                                    send_team_message: Some(AppSendMessage {
                                                        message: result.clone(),
                                                    }),
                                                    ..Default::default()
                                                };

                                                let mut buf = Vec::new();
                                                if send_request.encode(&mut buf).is_ok() {
                                                    let mut write_guard = write.lock().await;
                                                    if let Err(e) = write_guard.send(Message::Binary(buf)).await {
                                                        eprintln!("Failed to send: {}", e);
                                                    } else {
                                                        println!("[Sent to team chat]");
                                                    }
                                                }
                                            }

                                            // Clear the pending request
                                            *last_markers_request_seq.write().await = None;
                                        }

                                        // Store markers in state
                                        state_guard.markers = Some(markers);
                                    }

                                    if let Some(error) = response.error {
                                        if error.error.contains("not paired") ||
                                           error.error.contains("auth") ||
                                           error.error.contains("token") ||
                                           error.error.contains("unauthorized") {
                                            eprintln!("\nAuthentication Error: Token invalid or expired");
                                            eprintln!("Use Discord command /pair to re-pair with this server\n");
                                            return ReconnectAction::Stop;
                                        } else {
                                            eprintln!("API Error: {}", error.error);
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

                                                // Handle marker commands separately (async request)
                                                if command == "!showevents" || command == "!getmarkers" || command == "!debugmarkers" {
                                                    let is_show_events = command == "!showevents";
                                                    let is_debug = command == "!debugmarkers";
                                                    seq_counter += 1;
                                                    let markers_seq = seq_counter;
                                                    *last_markers_request_seq.write().await = Some((markers_seq, is_show_events, is_debug));

                                                    let markers_request = AppRequest {
                                                        seq: markers_seq,
                                                        player_id,
                                                        player_token,
                                                        entity_id: 0,
                                                        get_map_markers: Some(AppEmpty {}),
                                                        ..Default::default()
                                                    };

                                                    let mut buf = Vec::new();
                                                    if markers_request.encode(&mut buf).is_ok() {
                                                        let mut write_guard = write.lock().await;
                                                        if let Err(e) = write_guard.send(Message::Binary(buf)).await {
                                                            eprintln!("Failed to send markers request: {}", e);
                                                        }
                                                    }
                                                    continue; // Skip normal response handling
                                                }

                                                let state_guard = state.read().await;

                                                let response_text = match command.as_str() {
                                                    "!help" => Some("Commands: !time, !night, !day, !pop, !info, !showevents".to_string()),
                                                    "!time" => state_guard.get_current_time_info(),
                                                    "!pop" => state_guard.get_population_info(),
                                                    "!info" => state_guard.get_server_info(),
                                                    "!night" => state_guard.calculate_time_until_night(),
                                                    "!day" => state_guard.calculate_time_until_day(),
                                                    "!mapdebug" => {
                                                        if let (Some(map), Some(info)) = (&state_guard.map, &state_guard.info) {
                                                            Some(format!(
                                                                "Map Debug Info:\nMap Size: {}m\nOcean Margin: {}\nMonuments: {}\nMap Dimensions: {}x{}\n\nFirst monument:\n{} at ({:.1}, {:.1})",
                                                                info.map_size,
                                                                map.ocean_margin,
                                                                map.monuments.len(),
                                                                map.width,
                                                                map.height,
                                                                map.monuments.get(0).map(|m| m.token.as_str()).unwrap_or("none"),
                                                                map.monuments.get(0).map(|m| m.x).unwrap_or(0.0),
                                                                map.monuments.get(0).map(|m| m.y).unwrap_or(0.0)
                                                            ))
                                                        } else {
                                                            Some("Map data not loaded yet".to_string())
                                                        }
                                                    },
                                                    "!testmonuments" => {
                                                        if let (Some(map), Some(info)) = (&state_guard.map, &state_guard.info) {
                                                            // Skip train tunnels - filter them out
                                                            let mut non_tunnel_monuments: Vec<_> = map.monuments.iter()
                                                                .filter(|m| !m.token.contains("train_tunnel"))
                                                                .collect();

                                                            let total = non_tunnel_monuments.len();

                                                            // Rotate starting position based on timestamp for variety
                                                            let seed = std::time::SystemTime::now()
                                                                .duration_since(std::time::UNIX_EPOCH)
                                                                .unwrap()
                                                                .as_secs() as usize;

                                                            let offset = seed % total;
                                                            non_tunnel_monuments.rotate_left(offset);

                                                            let display_count = total.min(10);
                                                            let mut result = format!("Monument Grid Test (showing {} of {}):\n",
                                                                display_count, total);

                                                            for (i, monument) in non_tunnel_monuments.iter().take(display_count).enumerate() {
                                                                let grid = helpers::coords_to_grid(monument.x, monument.y, info.map_size);
                                                                let region = helpers::get_map_region(monument.x, monument.y, info.map_size);
                                                                result.push_str(&format!("{}. {} at {} ({}) - coords: ({:.0}, {:.0})\n",
                                                                    i + 1, monument.token, grid, region, monument.x, monument.y));
                                                            }
                                                            Some(result)
                                                        } else {
                                                            Some("Map data not loaded yet".to_string())
                                                        }
                                                    },
                                                    _ => Some("Unknown command. Type !help for available commands".to_string()),
                                                };

                                                drop(state_guard);

                                                if let Some(response) = response_text {
                                                    // Check if this is a debug command
                                                    let is_debug_command = command.as_str() == "!mapdebug" ||
                                                                          command.as_str() == "!testmonuments";

                                                    if is_debug_command {
                                                        // Debug commands - terminal only
                                                        println!("[Debug Output]\n{}", response);
                                                    } else {
                                                        // Normal commands - send to team chat
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
                                                                println!("[Sent to team chat]");
                                                            }
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
                        println!("Connection closed by server");
                        return ReconnectAction::Retry;
                    }
                    Err(e) => {
                        eprintln!("Connection error: {}", e);
                        return ReconnectAction::Retry;
                    }
                    _ => {}
                }
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Connection failed: {}", e);
            return ReconnectAction::Retry;
        }
    }
}
