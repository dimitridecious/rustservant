#!/usr/bin/env node

/**
 * Rust+ Pairing Capture Script
 *
 * This script listens for Rust+ pairing notifications and automatically
 * saves the server credentials to server_config.json
 *
 * Usage: node capture_pairing.js
 *
 * Then in Rust game: ESC -> Rust+ -> Pair with Server
 */

const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

const CONFIG_FILE = path.join(__dirname, 'servers.json');

function generateServerId(serverName) {
    return serverName
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '-')
        .replace(/^-+|-+$/g, '');
}

function loadServers() {
    if (fs.existsSync(CONFIG_FILE)) {
        try {
            const data = JSON.parse(fs.readFileSync(CONFIG_FILE, 'utf8'));
            return data;
        } catch (e) {
            console.error('Warning: Failed to parse existing servers.json:', e.message);
        }
    }
    return {
        active_server: null,
        servers: {}
    };
}

function saveServers(config) {
    try {
        fs.writeFileSync(CONFIG_FILE, JSON.stringify(config, null, 2));
    } catch (e) {
        console.error('ERROR: Failed to save servers.json:', e.message);
        throw e;
    }
}

console.log('[Pairing] Listening for server pairing notifications...\n');

// Run fcm-listen and capture output
const fcmListen = spawn('npx', ['@liamcottle/rustplus.js', 'fcm-listen'], {
    stdio: ['inherit', 'pipe', 'inherit']
});

let buffer = '';
let inObject = false;
let objectDepth = 0;
let currentObject = '';
let processedNotifications = new Set(); // Track processed notification IDs

fcmListen.stdout.on('data', (data) => {
    const output = data.toString();
    // Don't print raw output - only print when we actually process a server
    buffer += output;

    for (let i = 0; i < output.length; i++) {
        const char = output[i];

        if (char === '{') {
            if (!inObject) {
                inObject = true;
                currentObject = '';
            }
            objectDepth++;
            currentObject += char;
        } else if (char === '}') {
            currentObject += char;
            objectDepth--;

            if (objectDepth === 0 && inObject) {
                inObject = false;
                try {
                    // Extract notification ID to avoid processing duplicates
                    const idMatch = currentObject.match(/id:\s*['"]([^'"]+)['"]/);
                    let shouldProcess = true;

                    if (idMatch && idMatch[1]) {
                        const notificationId = idMatch[1];

                        // Skip if we've already processed this notification
                        if (processedNotifications.has(notificationId)) {
                            shouldProcess = false;
                        } else {
                            processedNotifications.add(notificationId);
                        }
                    }

                    // Extract the body field value using regex (it's valid JSON inside quotes)
                    // The value is a JSON object string that starts with { and ends with }
                    const bodyMatch = currentObject.match(/key:\s*['"]body['"]\s*,\s*value:\s*['"](\{.*\})['"]/s);


                    if (shouldProcess && bodyMatch && bodyMatch[1]) {
                        // The body value is already valid JSON, parse it directly
                        const serverInfo = JSON.parse(bodyMatch[1]);

                        // Check if this is a server pairing notification
                        if (serverInfo.type === 'server' && serverInfo.ip && serverInfo.port &&
                            serverInfo.playerId && serverInfo.playerToken) {

                            console.log('[Pairing] New server pairing detected!');
                            const serverId = generateServerId(serverInfo.name);
                            const serverConfig = {
                                name: serverInfo.name,
                                server_ip: serverInfo.ip,
                                server_port: parseInt(serverInfo.port),
                                player_id: serverInfo.playerId,
                                player_token: serverInfo.playerToken
                            };

                            const serversData = loadServers();
                            const isNewServer = !serversData.servers[serverId];

                            serversData.servers[serverId] = serverConfig;

                            if (serversData.active_server === null || isNewServer) {
                                serversData.active_server = serverId;
                            }

                            saveServers(serversData);

                            console.log('\n' + (isNewServer ? 'Added new server!' : 'Updated existing server!'));
                            console.log(`Saved to: ${CONFIG_FILE}`);
                            console.log('\nServer Configuration:');
                            console.log(`  Server ID:    ${serverId}`);
                            console.log(`  Server Name:  ${serverConfig.name}`);
                            console.log(`  IP:           ${serverConfig.server_ip}`);
                            console.log(`  Port:         ${serverConfig.server_port}`);
                            console.log(`  Player ID:    ${serverConfig.player_id}`);
                            console.log(`  Player Token: ${serverConfig.player_token.substring(0, 5)}...`);

                            if (serversData.active_server === serverId) {
                                console.log('\n  [ACTIVE SERVER]');
                            }

                            console.log('\nTotal servers: ' + Object.keys(serversData.servers).length);
                            console.log('You can now run: cargo run');
                            console.log('\nPress Ctrl+C to exit, or leave running to capture more servers.\n');
                        }
                    }
                } catch (e) {
                    console.error('[Pairing] Failed to parse notification:', e.message);
                }
                currentObject = '';
            }
        } else if (inObject) {
            currentObject += char;
        }
    }
});

fcmListen.on('close', (code) => {
    if (code !== 0) {
        console.error(`\nfcm-listen process exited with code ${code}`);
        console.error('Make sure you have run: npx @liamcottle/rustplus.js fcm-register');
    }
    process.exit(code);
});

// Handle Ctrl+C gracefully
process.on('SIGINT', () => {
    console.log('\n\nShutting down...');
    fcmListen.kill();
    process.exit(0);
});
