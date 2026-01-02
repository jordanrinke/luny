/** @toon
purpose: Sample JavaScript fixture for testing both CommonJS and ES module patterns.
    This file demonstrates various export styles used in JavaScript codebases.

when-editing:
    - !Keep both CommonJS and ES module patterns represented
    - Maintain JSDoc comments for type inference testing

invariants:
    - Functions should have clear parameter and return patterns
    - Both named and default exports must be present
*/

import { createServer } from 'http';
import { EventEmitter } from 'events';

// Class export
export class Logger extends EventEmitter {
    constructor(prefix = '') {
        super();
        this.prefix = prefix;
        this.logs = [];
    }

    /**
     * Log a message
     * @param {string} level - Log level
     * @param {string} message - Message to log
     */
    log(level, message) {
        const entry = {
            timestamp: new Date().toISOString(),
            level,
            message: this.prefix ? `[${this.prefix}] ${message}` : message,
        };
        this.logs.push(entry);
        this.emit('log', entry);
        console.log(`${entry.timestamp} [${level}] ${entry.message}`);
    }

    info(message) {
        this.log('INFO', message);
    }

    error(message) {
        this.log('ERROR', message);
    }

    warn(message) {
        this.log('WARN', message);
    }
}

// Function exports
export function formatDate(date) {
    return date.toISOString().split('T')[0];
}

export async function fetchData(url) {
    const response = await fetch(url);
    if (!response.ok) {
        throw new Error(`HTTP error: ${response.status}`);
    }
    return response.json();
}

// Arrow function export
export const debounce = (fn, delay) => {
    let timeoutId;
    return (...args) => {
        clearTimeout(timeoutId);
        timeoutId = setTimeout(() => fn(...args), delay);
    };
};

// Const exports
export const VERSION = '1.0.0';
export const DEFAULT_PORT = 3000;

// Object export
export const config = {
    apiUrl: 'https://api.example.com',
    timeout: 5000,
    retries: 3,
};

// Function using imports
export function startServer(port = DEFAULT_PORT) {
    const server = createServer((req, res) => {
        res.writeHead(200, { 'Content-Type': 'text/plain' });
        res.end('Hello World\n');
    });
    server.listen(port);
    return server;
}

// Default export
export default Logger;
