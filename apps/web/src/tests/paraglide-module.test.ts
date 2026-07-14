import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

describe('Paraglide module guard', () => {
	it('ensures generated message files remain ES modules', () => {
		const appRoot = process.cwd();

		execFileSync(process.execPath, [resolve(appRoot, 'scripts/ensure-paraglide-module.mjs')], {
			cwd: appRoot,
			stdio: 'pipe',
		});

		const messagesDir = resolve(appRoot, 'src/lib/paraglide/messages');
		const nonModules = readdirSync(messagesDir)
			.filter((file) => file.endsWith('.js'))
			.filter((file) => !/\bexport\b/.test(readFileSync(resolve(messagesDir, file), 'utf8')));

		expect(nonModules).toEqual([]);
	});
});
