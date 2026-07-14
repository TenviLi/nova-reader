import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';
import path from 'path';

export default defineConfig({
	plugins: [svelte({ hot: !process.env.VITEST })],
	test: {
		include: ['src/**/*.{test,spec}.{js,ts}'],
		environment: 'jsdom',
		globals: true,
		setupFiles: ['./src/tests/setup.ts'],
		coverage: {
			reporter: ['text', 'lcov'],
			include: ['src/lib/**/*.ts', 'src/lib/**/*.svelte'],
			exclude: ['src/tests/**', '**/*.d.ts']
		}
	},
	resolve: {
		conditions: ['browser'],
		alias: {
			'$lib': path.resolve('./src/lib'),
			'$lib/*': path.resolve('./src/lib/*'),
			'$components': path.resolve('./src/lib/components'),
			'$components/*': path.resolve('./src/lib/components/*'),
			'$services': path.resolve('./src/lib/services'),
			'$services/*': path.resolve('./src/lib/services/*'),
			'$types': path.resolve('./src/lib/types'),
			'$types/*': path.resolve('./src/lib/types/*'),
			'$stores': path.resolve('./src/lib/stores'),
			'$stores/*': path.resolve('./src/lib/stores/*'),
			'$utils': path.resolve('./src/lib/utils'),
			'$utils/*': path.resolve('./src/lib/utils/*'),
			'$app/stores': path.resolve('./src/tests/mocks/app-stores.ts'),
		}
	}
});
