/// <reference types="@sveltejs/kit" />
/// <reference no-default-lib="true"/>
/// <reference lib="esnext" />
/// <reference lib="webworker" />

declare const self: ServiceWorkerGlobalScope;

import { build, files, version } from '$service-worker';

const CACHE_NAME = `nova-reader-${version}`;
const ASSETS = [...build, ...files];

// Install: pre-cache static assets, then immediately activate (skip waiting)
self.addEventListener('install', (event: ExtendableEvent) => {
	event.waitUntil(
		caches.open(CACHE_NAME)
			.then((cache) => cache.addAll(ASSETS))
			.then(() => self.skipWaiting()) // Force activate immediately
	);
});

// Activate: clean up old caches, then claim all clients (no reload needed)
self.addEventListener('activate', (event: ExtendableEvent) => {
	event.waitUntil(
		caches.keys()
			.then((keys) => Promise.all(
				keys.filter((key) => key !== CACHE_NAME).map((key) => caches.delete(key))
			))
			.then(() => self.clients.claim()) // Take over all pages immediately
	);
});

// Fetch strategy: Network-first for everything except hashed static assets
self.addEventListener('fetch', (event: FetchEvent) => {
	const url = new URL(event.request.url);

	// Skip non-GET, cross-origin, and chrome-extension requests
	if (event.request.method !== 'GET') return;
	if (url.origin !== self.location.origin) return;

	// Hashed static assets (immutable) → cache-first
	// SvelteKit build outputs have content hashes in filenames
	if (ASSETS.includes(url.pathname)) {
		event.respondWith(
			caches.match(event.request).then((cached) => cached || fetch(event.request))
		);
		return;
	}

	// Everything else (pages, API, non-hashed assets) → network-first
	event.respondWith(
		fetch(event.request)
			.then((response) => {
				// Don't cache error responses or API
				if (!response.ok || url.pathname.startsWith('/api/')) {
					return response;
				}
				// Cache navigations for offline fallback
				if (event.request.mode === 'navigate') {
					const clone = response.clone();
					caches.open(CACHE_NAME).then((cache) => cache.put(event.request, clone));
				}
				return response;
			})
			.catch(() => {
				// Offline fallback: try cache
				return caches.match(event.request).then((cached) => {
					if (cached) return cached;
					// For navigations, return cached root as SPA fallback
					if (event.request.mode === 'navigate') {
						return caches.match('/') as Promise<Response>;
					}
					return new Response('Offline', { status: 503, headers: { 'Content-Type': 'text/plain' } });
				});
			})
	);
});
