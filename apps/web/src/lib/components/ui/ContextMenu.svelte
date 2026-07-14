<script lang="ts">
	import { onMount } from 'svelte';
	import type { Component } from 'svelte';

	interface MenuItem {
		label: string;
		icon?: Component;
		action: () => void;
		variant?: 'default' | 'danger';
		disabled?: boolean;
	}

	interface MenuGroup {
		items: MenuItem[];
	}

	let { visible = $bindable(), x, y, groups, onclose } = $props<{
		visible: boolean;
		x: number;
		y: number;
		groups: MenuGroup[];
		onclose: () => void;
	}>();

	let menuRef: HTMLDivElement | undefined = $state();

	// Adjust position to stay within viewport
	let adjustedX = $derived.by(() => {
		if (typeof window === 'undefined') return x;
		const menuWidth = 180;
		return Math.min(x, window.innerWidth - menuWidth - 8);
	});

	let adjustedY = $derived.by(() => {
		if (typeof window === 'undefined') return y;
		const menuHeight = groups.reduce((h: number, g: MenuGroup) => h + g.items.length * 36 + 8, 0);
		return Math.min(y, window.innerHeight - menuHeight - 8);
	});

	function handleClick(item: MenuItem) {
		if (item.disabled) return;
		item.action();
		onclose();
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') onclose();
	}

	function handleClickOutside(e: MouseEvent) {
		if (menuRef && !menuRef.contains(e.target as Node)) {
			onclose();
		}
	}

	$effect(() => {
		if (visible) {
			document.addEventListener('click', handleClickOutside, { capture: true });
			document.addEventListener('keydown', handleKeydown);
			return () => {
				document.removeEventListener('click', handleClickOutside, { capture: true });
				document.removeEventListener('keydown', handleKeydown);
			};
		}
	});
</script>

{#if visible}
	<div
		bind:this={menuRef}
		class="fixed z-[100] min-w-[180px] rounded-xl border border-ink-700/50 bg-ink-900 py-1 shadow-xl shadow-black/30 animate-fade-in"
		style="left: {adjustedX}px; top: {adjustedY}px;"
		role="menu"
	>
		{#each groups as group, gi}
			{#if gi > 0}
				<div class="my-1 h-px bg-ink-800/50"></div>
			{/if}
			{#each group.items as item}
				<button
					onclick={() => handleClick(item)}
					disabled={item.disabled}
					type="button"
					class="flex w-full items-center gap-2.5 px-3 py-2 text-sm transition-colors disabled:opacity-40
						{item.variant === 'danger' ? 'text-red-400 hover:bg-red-500/10' : 'text-ink-300 hover:bg-ink-800/50 hover:text-ink-100'}"
					role="menuitem"
				>
					{#if item.icon}
						{@const Icon = item.icon}
						<Icon size={15} strokeWidth={1.5} />
					{/if}
					{item.label}
				</button>
			{/each}
		{/each}
	</div>
{/if}
