# RustServant

**Work in Progress** - A Rust+ API integration tool that bridges Rust game servers with Discord for automated monitoring and management.

## Status

Currently in active development. The bot is not yet ready for general use.

**What's Working:**
- Rust+ WebSocket connection and authentication
- Multi-server configuration and switching
- Discord bot integration with slash commands
- Automatic server pairing via FCM notifications
- Server info commands (time, day/night cycles, server details)
- Map data and monument tracking
- Grid coordinate system (150m cells, supports AA-AB+ notation)
- Event marker fetching and display

**What's In Progress:**
- Event location commands (!where heli, !where cargo, etc.)
- Vending machine search by item name
- Distinguishing Bradley vs Heli crash sites

## Goals

Build a comprehensive Rust server companion that provides:

**Event Tracking**
- Locate Patrol Helicopter, Cargo Ship, Chinook, Locked Crates
- Track Traveling Vendor and Bradley APC
- Real-time event notifications

**Item & Economy**
- Search vending machines across the map
- Find items with pricing and stock levels
- Compare prices between vendors

**Team Management**
- Monitor team member locations and status
- Track who's online, alive, or dead
- View team positions on the map

**Smart Devices**
- Remote control switches, doors, turrets
- Monitor storage contents
- Manage alarms and automation

## Expected Output Examples

**Event Location:**
```
!showevents

Active Events (3):
Cargo Ship at Y0 (top right) - coords: (3745, 3706)
Traveling Vendor at G15 (left) - coords: (929, 1534)
Patrol Heli at R16 (center) - coords: (2584, 1345)
```

**Server Info:**
```
!info

Rust Server US West | 45/200 players | Map: 4000m | Seed: 1234567
```

**Grid System:**
Standard Rust grid notation (A-Z, then AA-AB+) with 150m cells:
- A0 = top left corner
- Z25 = bottom right (on 4000m map)

## Current Commands

**In-Game (Team Chat):**
- `!time` - Current server time
- `!night` - Time until nightfall
- `!day` - Time until daytime
- `!info` - Server information
- `!showevents` - List active events (development/testing)
- `!getmarkers` - Marker summary (development/testing)
- `!debugmarkers` - Detailed marker debug (development/testing)

**Discord:**
- `/servers` - List configured servers
- `/switch <server>` - Switch active server
- `/current` - Show current server
- `/remove <server>` - Remove server

## Technology Stack

- **Language:** Rust (2024 edition)
- **Protocol:** Protobuf via prost
- **WebSocket:** tokio-tungstenite
- **Discord:** serenity 0.12
- **Runtime:** tokio async

## Architecture

```
RustServant
├── Rust+ WebSocket Client
│   ├── Map marker tracking
│   ├── Team chat monitoring
│   └── Command processor
├── Discord Bot
│   ├── Slash commands
│   └── Server management
└── Automatic Pairing
    └── FCM notification capture
```

## License

MIT
