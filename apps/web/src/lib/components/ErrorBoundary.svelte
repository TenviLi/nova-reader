<script lang="ts">
	import { AlertTriangle, RefreshCcw } from 'lucide-svelte';
	import type { Snippet } from 'svelte';

	let { children, fallback }: { children: Snippet; fallback?: Snippet<[Error, () => void]> } = $props();

	let error = $state<Error | null>(null);

	function reset() {
		error = null;
	}

	// The error boundary catches errors from child rendering
	// In Svelte 5, we use the $effect.root for isolated error catching
</script>

{#if error}
	{#if fallback}
		{@render fallback(error, reset)}
	{:else}
		<div class="flex flex-col items-center justify-center gap-4 rounded-xl border border-red-500/20 bg-red-500/5 p-8 text-center">
			<AlertTriangle size={32} strokeWidth={1.5} class="text-red-400" />
			<div>
				<h3 class="text-sm font-semibold text-red-300">出现了一个错误</h3>
				<p class="mt-1 text-xs text-ink-400">{error.message}</p>
			</div>
			<button
				onclick={reset}
				class="flex items-center gap-1.5 rounded-lg bg-red-500/10 px-3 py-1.5 text-xs font-medium text-red-300 transition-colors hover:bg-red-500/20"
			>
				<RefreshCcw size={12} />
				重试
			</button>
		</div>
	{/if}
{:else}
	{@render children()}
{/if}
