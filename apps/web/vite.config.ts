import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { paraglideVitePlugin } from '@inlang/paraglide-js';
import { defineConfig } from 'vite';

export default defineConfig({
	envDir: '../../',
	plugins: [
		tailwindcss(),
		paraglideVitePlugin({
			project: './project.inlang',
			outdir: './src/lib/paraglide',
		}),
		sveltekit(),
	],
	server: {
		port: 5173,
		proxy: {
			// Proxy to Rust backend; SvelteKit +server.ts routes take priority
			'/api': {
				target: 'http://localhost:3000',
				changeOrigin: true,
				// Skip paths handled by SvelteKit server routes
				bypass(req) {
					if (req.url === '/api/chat' && req.method === 'POST') return req.url;
				}
			}
		}
	},
	ssr: {
		noExternal: ['@lucide/svelte', 'lucide-svelte']
	},
	build: {
		rollupOptions: {
			output: {
				manualChunks(id) {
					if (id.includes('node_modules/d3') || id.includes('node_modules/d3-force')) {
						return 'd3';
					}
					if (id.includes('node_modules/@xyflow')) {
						return 'xyflow';
					}
					if (id.includes('node_modules/@tanstack')) {
						return 'query';
					}
				}
			}
		}
	}
});
