<script lang="ts">
	import { Home, BookOpen, Search, Compass, Library as LibraryIcon } from 'lucide-svelte';
	import { page } from '$app/stores';

	const navItems = [
		{ href: '/', icon: Home, label: '首页' },
		{ href: '/library', icon: BookOpen, label: '所有书籍' },
		{ href: '/libraries', icon: LibraryIcon, label: '书库管理' },
		{ href: '/discover', icon: Compass, label: '探索' },
		{ href: '/search', icon: Search, label: '搜索' },
	];

	let currentPath = $derived($page.url.pathname);
</script>

<!-- Mobile bottom nav - only visible on small screens -->
<nav aria-label="移动主导航" class="fixed bottom-0 inset-x-0 z-50 border-t border-ink-800 bg-ink-950/95 backdrop-blur-md safe-area-pb md:hidden">
	<div class="flex items-center justify-around h-14">
		{#each navItems as item}
			{@const active = currentPath === item.href || (item.href !== '/' && currentPath.startsWith(`${item.href}/`))}
			<a
				href={item.href}
				class="flex flex-col items-center justify-center gap-0.5 w-full h-full transition-colors {active ? 'text-accent-400' : 'text-ink-500 active:text-ink-300'}"
			>
				<item.icon size={20} strokeWidth={active ? 2.5 : 1.5} />
				<span class="text-[10px] font-medium">{item.label}</span>
			</a>
		{/each}
	</div>
</nav>

<style>
	.safe-area-pb {
		padding-bottom: env(safe-area-inset-bottom, 0px);
	}
</style>
