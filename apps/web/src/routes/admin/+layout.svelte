<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { auth } from '$stores/auth.svelte';
	import { LayoutDashboard, Users, Clock, ScrollText, Brain, ChevronLeft, Settings2, HeartPulse } from 'lucide-svelte';

	let { children } = $props();

	// Admin guard: redirect if not admin role
	$effect(() => {
		if (!auth.loading && auth.user && auth.user.role !== 'admin') {
			goto('/');
		}
	});

	const navItems = [
		{ href: '/admin', label: '仪表盘', icon: LayoutDashboard, exact: true },
		{ href: '/admin/users', label: '用户', icon: Users },
		{ href: '/admin/jobs', label: '定时任务', icon: Clock },
		{ href: '/admin/logs', label: '系统日志', icon: ScrollText },
		{ href: '/admin/health', label: '数据健康', icon: HeartPulse },
		{ href: '/admin/ai-usage', label: 'AI 用量', icon: Brain },
		{ href: '/admin/ai-settings', label: 'AI 配置', icon: Settings2 },
	];

	function isActive(href: string, exact: boolean | undefined, pathname: string) {
		if (exact) return pathname === href;
		return pathname.startsWith(href);
	}
</script>

<div class="flex h-full flex-col gap-4 md:flex-row md:gap-6">
	<nav aria-label="管理导航" class="-mx-4 flex gap-2 overflow-x-auto border-b border-ink-800/50 px-4 pb-3 md:hidden">
		<a href="/" class="flex shrink-0 items-center gap-1.5 rounded-lg px-3 py-2 text-xs text-ink-500 transition-colors hover:bg-ink-900/50 hover:text-ink-300">
			<ChevronLeft class="h-3 w-3" />
			返回
		</a>
		{#each navItems as item}
			{@const active = isActive(item.href, item.exact, $page.url.pathname)}
			<a
				href={item.href}
				aria-current={active ? 'page' : undefined}
				class="flex shrink-0 items-center gap-2 rounded-lg px-3 py-2 text-sm transition-colors {active
					? 'bg-accent-500/10 font-medium text-accent-400'
					: 'text-ink-400 hover:bg-ink-900/50 hover:text-ink-200'}"
			>
				<item.icon class="h-4 w-4" />
				{item.label}
			</a>
		{/each}
	</nav>

	<!-- Admin side nav -->
	<nav aria-label="管理导航" class="hidden md:flex w-48 shrink-0 flex-col gap-1">
		<a href="/" class="flex items-center gap-1.5 mb-4 text-xs text-ink-500 hover:text-ink-300 transition-colors">
			<ChevronLeft class="w-3 h-3" />
			返回主页
		</a>
		{#each navItems as item}
			{@const active = isActive(item.href, item.exact, $page.url.pathname)}
			<a
				href={item.href}
				class="flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm transition-colors {active
					? 'bg-accent-500/10 text-accent-400 font-medium'
					: 'text-ink-400 hover:text-ink-200 hover:bg-ink-900/50'}"
			>
				<item.icon class="w-4 h-4" />
				{item.label}
			</a>
		{/each}
	</nav>

	<!-- Admin content -->
	<div class="flex-1 min-w-0 overflow-y-auto">
		{@render children()}
	</div>
</div>
