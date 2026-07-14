import { appendFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const messagesDir = fileURLToPath(new URL('../src/lib/paraglide/messages', import.meta.url));
const indexFile = join(messagesDir, '_index.js');

if (!existsSync(indexFile)) {
	mkdirSync(messagesDir, { recursive: true });
	writeFileSync(indexFile, '/* eslint-disable */\n');
}

function ensureModule(file) {
	const content = readFileSync(file, 'utf8');
	if (!/\bexport\b/.test(content)) {
		appendFileSync(file, '\nexport {};\n');
	}
}

for (const file of readdirSync(messagesDir)) {
	if (file.endsWith('.js')) {
		ensureModule(join(messagesDir, file));
	}
}
