<script lang="ts">
	import { Brain, BookOpen, Network, Loader2, ChevronRight } from 'lucide-svelte';

	interface KnowledgeNode {
		name: string;
		type: 'concept' | 'character' | 'place' | 'event' | 'ability';
		description: string;
		related_books: string[];
	}

	interface Props {
		query: string;
		summary?: string;
		nodes?: KnowledgeNode[];
		loading?: boolean;
		onNodeClick?: (node: KnowledgeNode) => void;
	}

	let { query, summary, nodes = [], loading = false, onNodeClick }: Props = $props();

	const TYPE_STYLES = {
		concept: { label: '概念', bg: 'bg-blue-50 text-blue-700 dark:bg-blue-900/20 dark:text-blue-300' },
		character: { label: '角色', bg: 'bg-purple-50 text-purple-700 dark:bg-purple-900/20 dark:text-purple-300' },
		place: { label: '地点', bg: 'bg-green-50 text-green-700 dark:bg-green-900/20 dark:text-green-300' },
		event: { label: '事件', bg: 'bg-amber-50 text-amber-700 dark:bg-amber-900/20 dark:text-amber-300' },
		ability: { label: '能力', bg: 'bg-red-50 text-red-700 dark:bg-red-900/20 dark:text-red-300' },
	};
</script>

{#if loading || summary || nodes.length > 0}
	<div class="rounded-xl border border-accent-100 bg-gradient-to-br from-accent-50/50 to-white p-5 dark:border-accent-800/30 dark:from-accent-950/20 dark:to-ink-900">
		<!-- Header -->
		<div class="mb-3 flex items-center gap-2">
			<Brain class="h-4 w-4 text-accent-500" />
			<h4 class="text-sm font-semibold text-ink-700 dark:text-ink-300">AI 知识摘要</h4>
			<span class="rounded-full bg-accent-100 px-2 py-0.5 text-xs text-accent-600 dark:bg-accent-900/30 dark:text-accent-400">
				{query}
			</span>
		</div>

		{#if loading}
			<div class="flex items-center gap-2 py-4 text-sm text-ink-400">
				<Loader2 class="h-4 w-4 animate-spin" />
				正在从知识图谱中分析...
			</div>
		{:else}
			<!-- Summary text -->
			{#if summary}
				<p class="mb-4 text-sm leading-relaxed text-ink-600 dark:text-ink-400">
					{summary}
				</p>
			{/if}

			<!-- Knowledge nodes -->
			{#if nodes.length > 0}
				<div class="space-y-2">
					<h5 class="flex items-center gap-1.5 text-xs font-medium text-ink-500">
						<Network class="h-3 w-3" />
						相关知识实体
					</h5>
					<div class="grid gap-2 sm:grid-cols-2">
						{#each nodes.slice(0, 6) as node}
							<button
								class="group flex items-start gap-2 rounded-lg border border-ink-100 bg-white/80 p-3 text-left transition-all hover:border-accent-200 hover:shadow-sm dark:border-ink-700 dark:bg-ink-800/50 dark:hover:border-accent-700"
								onclick={() => onNodeClick?.(node)}
							>
								<span class="mt-0.5 flex-shrink-0 rounded px-1.5 py-0.5 text-xs font-medium {TYPE_STYLES[node.type].bg}">
									{TYPE_STYLES[node.type].label}
								</span>
								<div class="min-w-0 flex-1">
									<p class="text-sm font-medium text-ink-700 dark:text-ink-300">{node.name}</p>
									<p class="mt-0.5 line-clamp-2 text-xs text-ink-400">{node.description}</p>
									{#if node.related_books.length > 0}
										<div class="mt-1.5 flex items-center gap-1 text-xs text-ink-300">
											<BookOpen class="h-3 w-3" />
											<span>出自 {node.related_books.length} 本书</span>
										</div>
									{/if}
								</div>
								<ChevronRight class="h-4 w-4 flex-shrink-0 text-ink-300 opacity-0 transition-opacity group-hover:opacity-100" />
							</button>
						{/each}
					</div>
				</div>
			{/if}
		{/if}
	</div>
{/if}
