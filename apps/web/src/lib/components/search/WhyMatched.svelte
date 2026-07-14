<script lang="ts">
	import { Info, ChevronDown } from 'lucide-svelte';

	interface Props {
		/** AI-generated explanation of why this result matched */
		explanation: string;
		/** Match factors (e.g., keyword overlap, semantic similarity, graph path) */
		factors?: Array<{
			type: 'keyword' | 'semantic' | 'graph' | 'entity' | 'context';
			label: string;
			weight: number;
		}>;
	}

	let { explanation, factors = [] }: Props = $props();

	let expanded = $state(false);

	const FACTOR_COLORS = {
		keyword: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300',
		semantic: 'bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-300',
		graph: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300',
		entity: 'bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-300',
		context: 'bg-pink-100 text-pink-700 dark:bg-pink-900/30 dark:text-pink-300',
	};
</script>

<div class="mt-2">
	<button
		class="flex items-center gap-1.5 text-xs text-ink-400 transition-colors hover:text-accent-500"
		onclick={() => expanded = !expanded}
	>
		<Info class="h-3 w-3" />
		<span>为什么匹配</span>
		<ChevronDown class="h-3 w-3 transition-transform {expanded ? 'rotate-180' : ''}" />
	</button>

	{#if expanded}
		<div class="mt-2 rounded-lg border border-ink-100 bg-ink-50/50 p-3 dark:border-ink-700 dark:bg-ink-800/50">
			<p class="text-xs leading-relaxed text-ink-500 dark:text-ink-400">
				{explanation}
			</p>
			{#if factors.length > 0}
				<div class="mt-2 flex flex-wrap gap-1.5">
					{#each factors as factor}
						<span class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs {FACTOR_COLORS[factor.type]}">
							{factor.label}
							<span class="opacity-60">{Math.round(factor.weight * 100)}%</span>
						</span>
					{/each}
				</div>
			{/if}
		</div>
	{/if}
</div>
