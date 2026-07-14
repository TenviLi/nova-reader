<script lang="ts">
	import { toggleMode, mode } from 'mode-watcher';
	import { navigating, page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { Search, Sun, Moon, Bell, User, LogOut, Settings, Shield, ChevronDown, ListChecks } from 'lucide-svelte';
	import { auth } from '$stores/auth.svelte';
	import * as DropdownMenu from '$components/ui/dropdown-menu';
	import { useUnreadNotificationCount } from '$lib/queries';

	let { sidebarCollapsed } = $props<{ sidebarCollapsed: boolean }>();

	let searchQuery = $state('');
	let searchOpen = $state(false);

	const unreadQuery = useUnreadNotificationCount();
	let unreadCount = $derived(unreadQuery.data ?? 0);

	// mode-watcher uses .current in Svelte 5 runes mode
	let isDark = $derived(mode.current === 'dark');

	// Route-based page title
	const routeTitles: Record<string, string> = {
		'/': '仪表盘',
		'/library': '所有书籍',
		'/reading': '继续阅读',
		'/series': '系列',
		'/collections': '书单',
		'/discover': '探索',
		'/search': '搜索',
		'/graph': '知识图谱',
		'/characters': '人物志',
		'/translate': '翻译工坊',
		'/writing': '创作工坊',
		'/stats': '统计与活动',
		'/persons': '作者',
		'/tasks': '任务队列',
		'/admin': '管理面板',
		'/notifications': '通知',
		'/libraries': '书库管理',
		'/library/duplicates': '重复检测',
		'/library/batch-edit': '批量编辑',
	};

	let pageTitle = $derived(() => {
		const path = $page.url.pathname;
		// Direct match
		if (routeTitles[path]) return routeTitles[path];
		// Prefix match for nested routes
		for (const [route, title] of Object.entries(routeTitles)) {
			if (route !== '/' && path.startsWith(`${route}/`)) return title;
		}
		return '';
	});

	async function handleLogout() {
		await auth.logout();
		goto('/login', { replaceState: true });
	}
</script>

<header class="relative flex h-14 shrink-0 items-center justify-between border-b border-ink-800/40 bg-ink-950/80 px-6 backdrop-blur-md">
	<!-- Navigation loading indicator -->
	{#if $navigating}
		<div class="absolute inset-x-0 top-0 h-0.5 overflow-hidden">
			<div class="h-full w-full bg-amber-500/80 animate-[loading-bar_1s_ease-in-out_infinite]"></div>
		</div>
	{/if}
	<!-- Left: Breadcrumb or title area -->
	<div class="flex items-center gap-4">
		<h2 class="text-sm font-medium text-ink-300">
			{pageTitle()}
		</h2>
	</div>

	<!-- Right: Actions -->
	<div class="flex items-center gap-2">
		<!-- Global Search Trigger -->
		<button
			onclick={() => window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', metaKey: true }))}
			aria-label="打开全局搜索"
			class="flex items-center gap-2 rounded-lg border border-ink-700/40 bg-ink-900/50 px-3 py-1.5 text-sm text-ink-500 transition-colors hover:border-ink-600 hover:bg-ink-800/50 hover:text-ink-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
			data-search-input
		>
			<Search size={14} strokeWidth={2} />
			<span class="hidden sm:inline">搜索…</span>
			<kbd class="ml-3 hidden sm:inline rounded border border-ink-700/60 px-1.5 py-0.5 text-[10px] font-mono text-ink-600">⌘K</kbd>
		</button>

		<!-- Theme Toggle -->
		<button
			onclick={() => toggleMode()}
			class="rounded-lg p-2 text-ink-500 hover:bg-ink-800/50 hover:text-ink-200 transition-colors"
			aria-label="切换主题"
		>
			{#if isDark}
				<Sun size={18} strokeWidth={1.5} />
			{:else}
				<Moon size={18} strokeWidth={1.5} />
			{/if}
		</button>

		<!-- Notifications -->
		<a href="/notifications" class="relative rounded-lg p-2 text-ink-500 hover:bg-ink-800/50 hover:text-ink-200 transition-colors" aria-label="通知">
			<Bell size={18} strokeWidth={1.5} />
			{#if unreadCount > 0}
				<span class="absolute -right-0.5 -top-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-accent-500 px-1 text-[10px] font-semibold text-white ring-2 ring-ink-950">
					{unreadCount > 99 ? '99+' : unreadCount}
				</span>
			{/if}
		</a>

		<!-- User Menu (shadcn DropdownMenu — portals to body, always on top) -->
		<DropdownMenu.Root>
		<DropdownMenu.Trigger
			aria-label="打开用户菜单"
			class="flex items-center gap-2 rounded-lg px-2 py-1.5 transition-colors hover:bg-ink-800/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
		>
				<div class="flex h-7 w-7 items-center justify-center rounded-full bg-accent-500/15 text-sm font-semibold text-accent-400 ring-1 ring-accent-500/20">
					{#if auth.user?.avatar_url}
						<img src={auth.user.avatar_url} alt="" class="h-full w-full rounded-full object-cover" />
					{:else}
						<User size={14} strokeWidth={2} />
					{/if}
				</div>
				<span class="hidden sm:inline text-sm text-ink-300 max-w-[100px] truncate">
					{auth.user?.display_name || auth.user?.username || '用户'}
				</span>
				<ChevronDown size={12} class="text-ink-500 hidden sm:block" />
			</DropdownMenu.Trigger>

			<DropdownMenu.Content align="end" class="w-52 bg-ink-900 border-ink-800/60">
				<DropdownMenu.Label class="px-3 py-2">
					<p class="text-sm font-medium text-ink-100 truncate">{auth.user?.display_name || auth.user?.username}</p>
					<p class="text-xs text-ink-500 truncate">@{auth.user?.username}</p>
				</DropdownMenu.Label>
				<DropdownMenu.Separator />
				<DropdownMenu.Group>
					<DropdownMenu.Item class="gap-2.5" onclick={() => goto('/settings')}>
						<Settings size={15} strokeWidth={1.5} />
						设置
					</DropdownMenu.Item>
					{#if auth.user?.role === 'admin'}
						<DropdownMenu.Item class="gap-2.5" onclick={() => goto('/tasks')}>
							<ListChecks size={15} strokeWidth={1.5} />
							任务队列
						</DropdownMenu.Item>
						<DropdownMenu.Item class="gap-2.5" onclick={() => goto('/admin')}>
							<Shield size={15} strokeWidth={1.5} />
							管理面板
						</DropdownMenu.Item>
					{/if}
				</DropdownMenu.Group>
				<DropdownMenu.Separator />
				<DropdownMenu.Item class="gap-2.5 text-red-400 data-highlighted:text-red-300 data-highlighted:bg-red-500/10" onclick={handleLogout}>
					<LogOut size={15} strokeWidth={1.5} />
					退出登录
				</DropdownMenu.Item>
			</DropdownMenu.Content>
		</DropdownMenu.Root>
	</div>
</header>
