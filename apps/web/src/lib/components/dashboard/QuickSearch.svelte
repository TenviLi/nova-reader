<script lang="ts">
	import { Search } from 'lucide-svelte';

	let query = $state('');
	let focused = $state(false);
</script>

<div class="relative">
	<div
		class="flex items-center gap-2 rounded-xl border bg-ink-900/50 px-4 py-2.5 transition-all duration-200 {focused ? 'border-accent-500/30' : ''} {!focused ? 'border-ink-700/50' : ''}"
		class:shadow-glow={focused}
		class:w-64={!focused}
		class:w-96={focused}
	>
		<Search size={15} strokeWidth={2} class="shrink-0 text-ink-400" />
		<input
			type="text"
			bind:value={query}
			onfocus={() => focused = true}
			onblur={() => focused = false}
			placeholder="搜索书名、作者、内容..."
			class="w-full bg-transparent text-sm text-ink-100 placeholder-ink-500 outline-none"
			data-search-input
		/>
		{#if !focused}
			<kbd class="shrink-0 rounded border border-ink-600 px-1.5 py-0.5 text-[10px] text-ink-500">⌘K</kbd>
		{/if}
	</div>
</div>
