<script lang="ts">
	/**
	 * Recursive tree node component for the Trope Ontology tree.
	 * Each node can expand to show children and linked chunks.
	 */
	import { ChevronRight, FileText, Layers } from 'lucide-svelte';
	import OntologyTreeNode from './OntologyTreeNode.svelte';

	interface TropeNode {
		id: string;
		parent_id: string | null;
		label: string;
		description: string | null;
		level: number;
		cluster_size: number;
		stability: number;
		attributes: Record<string, unknown>;
		domain: string;
		is_leaf: boolean;
		children_count: number;
	}

	interface ChunkRef {
		id: string;
		book_id: string;
		book_title?: string;
		chapter_index: number;
		chunk_index: number;
		membership_score: number;
		text?: string;
	}

	let {
		node,
		allNodes = [],
		depth = 0,
		onSelect,
		onChunkClick,
	}: {
		node: TropeNode;
		allNodes: TropeNode[];
		depth?: number;
		onSelect?: (node: TropeNode) => void;
		onChunkClick?: (chunk: ChunkRef) => void;
	} = $props();

	function initiallyExpanded() {
		return depth < 1;
	}

	let expanded = $state(initiallyExpanded());
	let chunksLoaded = $state(false);
	let chunks = $state<ChunkRef[]>([]);
	let loadingChunks = $state(false);

	let children = $derived(allNodes.filter(n => n.parent_id === node.id));
	let hasChildren = $derived(children.length > 0 || node.children_count > 0);

	function toggle() {
		expanded = !expanded;
	}

	async function loadChunks() {
		if (chunksLoaded) return;
		loadingChunks = true;
		try {
			const { api } = await import('$services/api');
			chunks = await api.get(`/ontology/tree/${node.id}/chunks?limit=5`);
			chunksLoaded = true;
		} catch (e) {
			console.error('Failed to load chunks:', e);
		} finally {
			loadingChunks = false;
		}
	}

	const domainColors: Record<string, string> = {
		worldbuilding: 'bg-emerald-500/20 text-emerald-400',
		power_system: 'bg-amber-500/20 text-amber-400',
		relationship: 'bg-blue-500/20 text-blue-400',
		trope: 'bg-violet-500/20 text-violet-400',
		tone: 'bg-cyan-500/20 text-cyan-400',
		general: 'bg-ink-700 text-ink-300',
	};
</script>

<div class="select-none" style="padding-left: {depth * 16}px">
	<!-- Node row -->
	<div class="group flex items-center gap-1.5 py-1 px-2 rounded-md hover:bg-ink-800/60 transition-colors">
		<!-- Expand arrow -->
		<button
			type="button"
			class="w-4 h-4 flex items-center justify-center shrink-0"
			onclick={toggle}
			disabled={!hasChildren}
			aria-label={expanded ? '收起子节点' : '展开子节点'}
			aria-expanded={expanded}
		>
			{#if hasChildren}
				<ChevronRight
					class="w-3.5 h-3.5 text-ink-500 transition-transform duration-150 {expanded ? 'rotate-90' : ''}"
				/>
			{/if}
		</button>

		<!-- Node content -->
		<button
			type="button"
			class="flex-1 flex items-center gap-2 text-left min-w-0"
			onclick={() => { onSelect?.(node); loadChunks(); }}
		>
			<Layers class="w-3.5 h-3.5 text-amber-400 shrink-0" />
			<span class="text-sm text-ink-200 truncate font-medium">{node.label}</span>
			<span class="text-[10px] px-1.5 py-0.5 rounded-full {domainColors[node.domain] ?? domainColors.general} shrink-0">
				{node.domain}
			</span>
			<span class="text-[10px] text-ink-500 ml-auto shrink-0">
				{node.cluster_size}
			</span>
		</button>
	</div>

	<!-- Expanded content -->
	{#if expanded}
		<!-- Children -->
		{#each children as child (child.id)}
			<OntologyTreeNode
				node={child}
				{allNodes}
				depth={depth + 1}
				{onSelect}
				{onChunkClick}
			/>
		{/each}

		<!-- Linked chunks (shown when node is selected) -->
		{#if chunksLoaded && chunks.length > 0}
			<div class="ml-8 mt-1 mb-2 space-y-1">
				{#each chunks as chunk}
					<button
						type="button"
						class="w-full text-left flex items-start gap-2 px-2 py-1.5 rounded bg-ink-900/50 border border-ink-800 hover:border-amber-500/30 transition-colors"
						onclick={() => onChunkClick?.(chunk)}
					>
						<FileText class="w-3 h-3 text-ink-500 mt-0.5 shrink-0" />
						<div class="min-w-0 flex-1">
							<div class="flex items-center gap-2 text-[10px] text-ink-500">
								<span>Ch.{chunk.chapter_index}</span>
								<span>·</span>
								<span>Chunk {chunk.chunk_index}</span>
								<span class="ml-auto text-amber-400/70">{(chunk.membership_score * 100).toFixed(0)}%</span>
							</div>
							{#if chunk.text}
								<p class="text-xs text-ink-400 line-clamp-2 mt-0.5">{chunk.text}</p>
							{/if}
						</div>
					</button>
				{/each}
			</div>
		{/if}
	{/if}
</div>
