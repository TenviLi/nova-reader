<script lang="ts">
	import { Layers, BookOpen, Users, Brain, Library } from 'lucide-svelte';

	interface ClusterGroup {
		id: string;
		label: string;
		type: 'chapters' | 'entities' | 'concepts' | 'similar_books';
		count: number;
		preview: string[];
	}

	interface Props {
		clusters: ClusterGroup[];
		activeCluster?: string;
		onClusterSelect?: (clusterId: string) => void;
	}

	let { clusters = [], activeCluster, onClusterSelect }: Props = $props();

	const CLUSTER_CONFIG = {
		chapters: { icon: BookOpen, color: 'border-blue-200 bg-blue-50 text-blue-700 dark:border-blue-800 dark:bg-blue-900/20 dark:text-blue-300' },
		entities: { icon: Users, color: 'border-purple-200 bg-purple-50 text-purple-700 dark:border-purple-800 dark:bg-purple-900/20 dark:text-purple-300' },
		concepts: { icon: Brain, color: 'border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-800 dark:bg-emerald-900/20 dark:text-emerald-300' },
		similar_books: { icon: Library, color: 'border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-800 dark:bg-amber-900/20 dark:text-amber-300' },
	};
</script>

{#if clusters.length > 0}
	<div class="mb-4">
		<div class="mb-2 flex items-center gap-2">
			<Layers class="h-4 w-4 text-ink-400" />
			<span class="text-xs font-medium text-ink-500 dark:text-ink-400">按类型浏览</span>
		</div>
		<div class="flex flex-wrap gap-2">
			{#each clusters as cluster (cluster.id)}
				{@const config = CLUSTER_CONFIG[cluster.type]}
				{@const Icon = config.icon}
				<button
					class="flex items-center gap-2 rounded-lg border px-3 py-2 text-sm transition-all {
						activeCluster === cluster.id
							? 'ring-2 ring-accent-300 ring-offset-1 dark:ring-offset-ink-900 ' + config.color
							: 'border-ink-200 bg-white text-ink-600 hover:border-ink-300 dark:border-ink-700 dark:bg-ink-800 dark:text-ink-400 dark:hover:border-ink-600'
					}"
					onclick={() => onClusterSelect?.(cluster.id)}
					type="button"
					aria-pressed={activeCluster === cluster.id}
				>
					<Icon class="h-4 w-4" />
					<span class="font-medium">{cluster.label}</span>
					<span class="rounded-full bg-ink-100 px-1.5 py-0.5 text-xs dark:bg-ink-700">{cluster.count}</span>
				</button>
			{/each}
		</div>

		<!-- Preview of active cluster -->
		{#if activeCluster}
			{@const active = clusters.find(c => c.id === activeCluster)}
			{#if active && active.preview.length > 0}
				<div class="mt-2 rounded-lg border border-ink-100 bg-ink-50/50 p-3 dark:border-ink-700 dark:bg-ink-800/50">
					<ul class="space-y-1">
						{#each active.preview.slice(0, 4) as item}
							<li class="text-xs text-ink-500 dark:text-ink-400">• {item}</li>
						{/each}
					</ul>
				</div>
			{/if}
		{/if}
	</div>
{/if}
