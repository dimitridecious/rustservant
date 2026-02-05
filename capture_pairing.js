#!/usr/bin/env node

/**
 * Rust+ Pairing Capture Script
 *
 * Usage: node capture_pairing.js [--timeout=60]
 *
 * Then in Rust game: ESC -> Rust+ -> Pair with Server
 *
 * Exit codes:
 *   0 = Success (server paired)
 *   1 = Error
 *   124 = Timeout
 */

const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

const CONFIG_FILE = path.join(__dirname, 'servers.json');
const HISTORY_FILE = path.join(__dirname, '.pairing_history.json');

// Parse command line arguments
const args = process.argv.slice(2);
let timeout = null;

for (const arg of args) {
    if (arg.startsWith('--timeout=')) {
        timeout = parseInt(arg.split('=')[1], 10);
        if (isNaN(timeout) || timeout <= 0) {
            console.error('ERROR: Invalid timeout value');
            process.exit(1);
        }
    }
}

// Load and clean pairing history
function loadPairingHistory() {
    if (!fs.existsSync(HISTORY_FILE)) {
        return new Set();
    }

    try {
        const data = JSON.parse(fs.readFileSync(HISTORY_FILE, 'utf8'));
        const oneHourAgo = Date.now() - (60 * 60 * 1000);

        // Clean old entries (older than 1 hour)
        const cleaned = Object.entries(data)
            .filter(([_, timestamp]) => timestamp > oneHourAgo)
            .reduce((acc, [id, timestamp]) => {
                acc[id] = timestamp;
                return acc;
            }, {});

        // Save cleaned data back
        fs.writeFileSync(HISTORY_FILE, JSON.stringify(cleaned));

        return new Set(Object.keys(cleaned));
    } catch (e) {
        return new Set();
    }
}

function savePairingHistory(processedIds) {
    try {
        const data = {};
        const now = Date.now();
        processedIds.forEach(id => {
            data[id] = now;
        });
        fs.writeFileSync(HISTORY_FILE, JSON.stringify(data));
    } catch (e) {
        // Silent fail
    }
}

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

console.log('PAIRING_LISTENING');

// Set up timeout if specified
let timeoutHandle = null;
if (timeout) {
    timeoutHandle = setTimeout(() => {
        console.log('PAIRING_TIMEOUT');
        fcmListen.kill();
        process.exit(124);
    }, timeout * 1000);
}

// Run fcm-listen and capture output
const fcmListen = spawn('npx', ['@liamcottle/rustplus.js', 'fcm-listen'], {
    stdio: ['inherit', 'pipe', 'inherit']
});

let buffer = '';
let inObject = false;
let objectDepth = 0;
let currentObject = '';
let processedNotifications = loadPairingHistory(); // Load previously processed notification IDs

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
                            savePairingHistory(processedNotifications);
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

                            // Extract timestamp from the notification (FCM includes this)
                            const timestampMatch = currentObject.match(/key:\s*['"]google.sent_time['"]\s*,\s*value:\s*['"](\d+)['"]/);
                            let isRecentNotification = true;

                            if (timestampMatch && timestampMatch[1]) {
                                const notificationTime = parseInt(timestampMatch[1], 10);
                                const currentTime = Date.now();
                                const ageMinutes = (currentTime - notificationTime) / 1000 / 60;

                                // Ignore notifications older than 2 minutes (likely FCM retries from old pairing)
                                if (ageMinutes > 2) {
                                    isRecentNotification = false;
                                }
                            }

                            if (!isRecentNotification) {
                                // Skip old lingering FCM notification
                            } else {

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
                            const isFirstServer = Object.keys(serversData.servers).length === 0;

                            serversData.servers[serverId] = serverConfig;

                            // Only set as active if it's the very first server
                            if (isFirstServer) {
                                serversData.active_server = serverId;
                            }

                            saveServers(serversData);

                            // Output machine-readable success message
                            console.log('PAIRING_SUCCESS');
                            console.log(`SERVER_ID:${serverId}`);
                            console.log(`SERVER_NAME:${serverConfig.name}`);
                            console.log(`SERVER_ADDRESS:${serverConfig.server_ip}:${serverConfig.server_port}`);
                            console.log(`IS_NEW:${isNewServer}`);

                            // Clear timeout and exit successfully
                            if (timeoutHandle) {
                                clearTimeout(timeoutHandle);
                            }
                            fcmListen.kill();
                            process.exit(0);
                            }
                        }
                    }
                } catch (e) {
                    // Silent fail for parse errors
                }
                currentObject = '';
            }
        } else if (inObject) {
            currentObject += char;
        }
    }
});

fcmListen.on('close', (code) => {
    if (timeoutHandle) {
        clearTimeout(timeoutHandle);
    }

    if (code !== 0 && code !== null) {
        console.log('PAIRING_ERROR:FCM_LISTEN_FAILED');
        console.error('Make sure you have run: npx @liamcottle/rustplus.js fcm-register');
        process.exit(1);
    }
});

// Handle Ctrl+C gracefully
process.on('SIGINT', () => {
    if (timeoutHandle) {
        clearTimeout(timeoutHandle);
    }
    console.log('PAIRING_CANCELLED');
    fcmListen.kill();
    process.exit(130);
});
