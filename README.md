# RustServant

A Discord bot that integrates with Rust game servers via the RustPlus API for server monitoring and management.

## Prerequisites

- **Rust** (latest stable) - [Install](https://rustup.rs/)
- **Node.js** (v16+) and npm - [Install](https://nodejs.org/)
- **Discord Bot Token** - [Create a bot](https://discord.com/developers/applications)
- **Rust+ Companion App** credentials

## Installation

### 1. Clone the repository

```bash
git clone git@github.com:dimitridecious/rustservant.git
cd rustservant
```

### 2. Install Rust dependencies

```bash
cargo build
```

### 3. Install Node.js dependencies (for pairing capture)

```bash
npm install
```

## Configuration

### 1. Discord Configuration

Create `discord_config.json` from the example:

```bash
cp discord_config.json.example discord_config.json
```

Edit `discord_config.json` with your values:
- `bot_token`: Your Discord bot token
- `application_id`: Your Discord application ID
- `guild_id`: Your Discord server (guild) ID

### 2. Server Configuration

Create `server_config.json` from the example:

```bash
cp server_config.json.example server_config.json
```

Edit `server_config.json` with your values:
- `server_ip`: Your Rust server IP address
- `server_port`: Your Rust server port (default: 28017)
- `player_id`: Your Steam ID
- `player_token`: Your player token from Rust+

### 3. Multiple Servers (Optional)

Create `servers.json` from the example:

```bash
cp servers.json.example servers.json
```

Configure multiple servers with their respective credentials.

### 4. RustPlus Configuration

Create `rustplus.config.json` from the example:

```bash
cp rustplus.config.json.example rustplus.config.json
```

This file contains FCM credentials and authentication tokens for the RustPlus API.

## Getting Rust+ Credentials

To get your player token and other credentials:

1. Run the pairing capture script:
   ```bash
   npm run pair
   ```

2. Pair with your server using the Rust+ companion app on your phone

3. The script will capture and display the pairing credentials

4. Copy the credentials to your config files

## Running the Bot

### Development

```bash
cargo run
```

### Production

```bash
cargo build --release
./target/release/rustservant
```

## Project Structure

- `src/` - Rust source code
  - `main.rs` - Main entry point
  - `discord/` - Discord bot integration
- `capture_pairing.js` - Node.js script for capturing Rust+ pairing
- `rustplus.proto` - Protocol buffer definitions for RustPlus API
- `*.json.example` - Example configuration files

## Important Security Notes

- **Never commit** your actual config files (`*.json`) to git
- All sensitive configuration files are excluded via `.gitignore`
- Only commit the `.example` files as templates
- Keep your Discord bot token and player tokens secure

## Troubleshooting

### Bot doesn't connect to Discord
- Verify your bot token in `discord_config.json`
- Check that the bot has proper permissions in your Discord server
- Ensure the application_id and guild_id are correct

### Can't connect to Rust server
- Verify server IP and port are correct
- Check that your player_id and player_token are valid
- Ensure the Rust server has Rust+ enabled

### Pairing capture not working
- Make sure Node.js dependencies are installed (`npm install`)
- Check that you're using the correct server details
- Try re-pairing with the Rust+ app

## License

MIT
