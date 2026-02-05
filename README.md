# RustServant

**Work in Progress** - A Rust+ API integration tool that bridges Rust game servers with Discord for automated monitoring and management.

## Status

Currently in active development. The bot is not yet ready for general use.

**What's Working:**
- Rust+ WebSocket connection and authentication
- Auto-reconnect with exponential backoff (handles network drops)
- Multi-server configuration and switching
- Discord bot integration with slash commands
- On-demand server pairing via FCM notifications (60-second listener)
- FCM notification deduplication and time-based filtering
- Server info commands (time, day/night cycles, population, server name)
- Map data and monument tracking
- Grid coordinate system (150m cells, supports AA-AB+ notation)
- Event marker fetching and display with grid locations

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
- Track who's online
- Notify when player dies, with location of death

## Expected Output Examples

**Time:**
```
!time
14:23 (Day) - Night in 42m 34s
```

**Population:**
```
!pop
Players: 45/200 (3 in queue)
```

**Server Info:**
```
!info
Rustafied.com - US Monday
```

**Event Location:**
```
!showevents

Active Events (3):
Cargo Ship at Y0 (top right) - coords: (3745, 3706)
Traveling Vendor at G15 (left) - coords: (929, 1534)
Patrol Heli at R16 (center) - coords: (2584, 1345)
```

**Grid System:**
Standard Rust grid notation (A-Z, then AA-AB+) with 150m cells:
- A0 = top left corner
- Z25 = bottom right (on 4000m map)

## Current Commands

**In-Game (Team Chat):**
- `!help` - Show available commands
- `!time` - Current time and next day/night cycle
- `!night` - Time until nightfall
- `!day` - Time until daytime
- `!pop` - Player count and queue
- `!info` - Server name
- `!showevents` - List active events with locations

**Debug Commands (Terminal Output Only):**
- `!getmarkers` - Marker summary counts
- `!debugmarkers` - Detailed marker information
- `!mapdebug` - Map metadata and dimensions
- `!testmonuments` - Monument grid coordinate testing

**Discord:**
- `/help` - Show all available commands (including debug commands)
- `/pair` - Pair with a new server (60 second listener)
- `/servers` - List configured servers
- `/current` - Show current server
- `/switch <server>` - Switch active server
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
│   ├── Auto-reconnect with exponential backoff
│   ├── Map marker tracking
│   ├── Team chat monitoring
│   └── Command processor
├── Discord Bot
│   ├── Slash commands
│   ├── Server management
│   └── On-demand pairing trigger
└── Pairing System (On-Demand)
    ├── 60-second FCM listener
    ├── Notification deduplication
    └── Time-based filtering (2-minute window)
```

## License

MIT
