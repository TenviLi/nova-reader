<script lang="ts">
	import { QueryClientProvider, QueryClient } from '@tanstack/svelte-query';
	import { SvelteQueryDevtools } from '@tanstack/svelte-query-devtools';
	import { ModeWatcher } from 'mode-watcher';
	import { Toaster } from 'svelte-sonner';
	import { onMount } from 'svelte';
	import { dev } from '$app/environment';
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import { auth } from '$stores/auth.svelte';
	import { featureFlags } from '$stores/settings.svelte';
	import { api } from '$lib/services/api';
	import '../app.css';
	import Sidebar from '$components/layout/Sidebar.svelte';
	import TopBar from '$components/layout/TopBar.svelte';
	import MobileNav from '$components/layout/MobileNav.svelte';
	import CommandPalette from '$components/CommandPalette.svelte';
	import KeyboardShortcuts from '$components/KeyboardShortcuts.svelte';

	let { children } = $props();

	let sidebarCollapsed = $state(false);

	// Public routes that don't require authentication
	const PUBLIC_ROUTES = ['/login', '/setup', '/share'];
	let currentPath = $derived($page.url.pathname);
	let isPublicRoute = $derived(PUBLIC_ROUTES.some(r => currentPath.startsWith(r)));
	let isReaderRoute = $derived(currentPath.startsWith('/reading/'));

	// Initialize auth on app load and handle redirects
	onMount(async () => {
		await auth.init();

		// Load feature flags from server after auth
		if (auth.isAuthenticated) {
			featureFlags.load(() => api.getSettings());
		}

		// If not authenticated and not on a public route, redirect to login
		if (!auth.isAuthenticated && !isPublicRoute) {
			// Check if this is a first-time setup (no users exist)
			try {
				const status = await fetch('/api/health/setup-status').then(r => r.json());
				if (status?.needs_setup) {
					goto('/setup', { replaceState: true });
				} else {
					goto('/login', { replaceState: true });
				}
			} catch {
				// If setup-status endpoint doesn't exist, just go to login
				goto('/login', { replaceState: true });
			}
		}
	});

	// Reactive auth guard — when auth finishes loading and user is null, redirect
	$effect(() => {
		if (!auth.loading && !auth.isAuthenticated && !isPublicRoute) {
			goto('/login', { replaceState: true });
		}
	});

	const queryClient = new QueryClient({
		defaultOptions: {
			queries: {
				staleTime: 1000 * 60 * 5,
				gcTime: 1000 * 60 * 30,
				refetchOnWindowFocus: true,
				retry: (failureCount, error: Error) => {
					// Don't retry on 401, 403, 404 — these are not transient
					const status = (error as { status?: number }).status;
					if (status === 401 || status === 403 || status === 404) {
						return false;
					}
					return failureCount < 2;
				},
			},
		},
	});

	// Show loading skeleton while auth is initializing
	let showApp = $derived(!auth.loading && (auth.isAuthenticated || isPublicRoute));
</script>

<ModeWatcher defaultMode="dark" lightClassNames={["light"]} />
<QueryClientProvider client={queryClient}>
	{#if dev}<SvelteQueryDevtools client={queryClient} />{/if}
	{#if showApp}
		{#if isPublicRoute}
			<!-- Public routes render without chrome -->
			{@render children()}
		{:else if isReaderRoute}
			<!-- Reader route: fullscreen, no sidebar/topbar -->
			{@render children()}
		{:else}
			<div class="flex h-screen overflow-hidden bg-ink-950">
				<!-- Desktop sidebar (hidden on mobile) -->
				<div class="hidden md:block">
					<Sidebar bind:collapsed={sidebarCollapsed} />
				</div>

				<div class="flex flex-1 flex-col overflow-hidden">
					<TopBar {sidebarCollapsed} />

					<main class="flex-1 overflow-y-auto pb-16 md:pb-0">
							{@render children()}
					</main>
				</div>
			</div>

			<!-- Mobile bottom nav -->
			<MobileNav />

			<CommandPalette />
			<KeyboardShortcuts />
		{/if}
	{:else}
		<!-- Auth loading state -->
		<div class="flex h-screen items-center justify-center bg-ink-950">
			<div class="text-center">
				<div class="inline-flex items-center justify-center w-12 h-12 rounded-2xl bg-gradient-to-br from-amber-500 to-amber-700 mb-4 animate-pulse">
					<svg class="w-6 h-6 text-ink-950" fill="none" viewBox="0 0 24 24" stroke="currentColor">
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" />
					</svg>
				</div>
				<p class="text-sm text-ink-500">正在加载...</p>
			</div>
		</div>
	{/if}
</QueryClientProvider>

<Toaster
	position="bottom-right"
	richColors
	closeButton
/>
