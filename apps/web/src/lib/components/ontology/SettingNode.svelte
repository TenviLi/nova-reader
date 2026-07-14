<script lang="ts">
	/**
	 * Recursive Tree Node with full CRUD, drag-drop, scan, evolve.
	 * Each node is a "concept anchor" — user-defined or AI-discovered.
	 */
	import { ChevronRight, ChevronDown, GripVertical, Scan, Sparkles, Plus, Pencil, Trash2 } from 'lucide-svelte';
	import { api } from '$services/api';
	import { dndzone } from 'svelte-dnd-action';
	import EvolveProposals from './EvolveProposals.svelte';
	import SettingNode from './SettingNode.svelte';

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

	interface MatchedChunk {
		point_id: number;
		book_id: string;
		book_title: string;
		chapter_index: number;
		chunk_index: number;
		text: string;
		score: number;
		language: string | null;
	}

	let {
		node,
		allNodes = [],
		depth = 0,
		selectedId = $bindable(''),
		onRefresh,
	}: {
		node: TropeNode;
		allNodes: TropeNode[];
		depth?: number;
		selectedId?: string;
		onRefresh?: () => void;
	} = $props();

	function initiallyExpanded() {
		return depth < 1;
	}

	function focusOnMount(node: HTMLInputElement) {
		node.focus();
		return {};
	}

	let expanded = $state(initiallyExpanded());
	let editing = $state(false);
	let editLabel = $state('');
	let scanning = $state(false);
	let evolving = $state(false);
	let scanResults = $state<MatchedChunk[]>([]);
	let showResults = $state(false);
	let showActions = $state(false);
	let addingChild = $state(false);
	let newChildLabel = $state('');
	let dragOver = $state(false);

	// Evolve proposals
	let evolveProposals = $state<{ label: string; description: string | null; evidence_count: number; sample_text: string; auto_created: boolean; created_id: string | null }[] | null>(null);
	let evolveEvidence = $state(0);

	let children = $derived(allNodes.filter(n => n.parent_id === node.id));
	let dndChildren = $state<TropeNode[]>([]);
	$effect(() => { dndChildren = [...children]; });
	let hasChildren = $derived(children.length > 0);
	let isSelected = $derived(selectedId === node.id);

	const langFlags: Record<string, string> = {
		zh: '🇨🇳',
		ja: '🇯🇵',
		en: '🇺🇸',
		ko: '🇰🇷',
		unknown: '🌐',
	};

	// ─── Actions ─────────────────────────────────────────────────────────────

	function startEdit() {
		editLabel = node.label;
		editing = true;
	}

	async function saveEdit() {
		if (!editLabel.trim()) return;
		try {
			await api.put(`/ontology/nodes/${node.id}`, { label: editLabel.trim() });
			node.label = editLabel.trim();
			editing = false;
			onRefresh?.();
		} catch (e) {
			console.error('Edit failed:', e);
		}
	}

	async function deleteNode() {
		if (!confirm(`确定删除「${node.label}」？子节点将上移到父级。`)) return;
		try {
			await api.del(`/ontology/nodes/${node.id}`);
			onRefresh?.();
		} catch (e) {
			console.error('Delete failed:', e);
		}
	}

	async function scanNode() {
		scanning = true;
		showResults = true;
		try {
			const result = await api.post<{ matched_chunks: typeof scanResults; total_matched: number }>(`/ontology/nodes/${node.id}/scan`, {
				limit: 30,
				threshold: 0.4,
			});
			scanResults = result.matched_chunks;
			node.cluster_size = result.total_matched;
		} catch (e) {
			console.error('Scan failed:', e);
		} finally {
			scanning = false;
		}
	}

	async function evolveNode() {
		evolving = true;
		try {
			const result = await api.post<{ proposed_children: typeof evolveProposals; evidence_chunks: typeof evolveEvidence }>(`/ontology/nodes/${node.id}/evolve`, {
				max_children: 5,
				min_evidence: 3,
			});
			// Show proposals for accept/reject instead of auto-committing
			if (result.proposed_children && result.proposed_children.length > 0) {
				evolveProposals = result.proposed_children;
				evolveEvidence = result.evidence_chunks;
				expanded = true;
			} else {
				onRefresh?.();
				expanded = true;
			}
		} catch (e) {
			console.error('Evolve failed:', e);
			alert((e as Error).message || '进化失败，可能证据不够（先扫描）');
		} finally {
			evolving = false;
		}
	}

	function handleEvolveDecisions() {
		evolveProposals = null;
		onRefresh?.();
	}

	async function addChild() {
		if (!newChildLabel.trim()) return;
		try {
			await api.post('/ontology/nodes', {
				label: newChildLabel.trim(),
				parent_id: node.id,
				domain: node.domain,
			});
			newChildLabel = '';
			addingChild = false;
			onRefresh?.();
			expanded = true;
		} catch (e) {
			console.error('Add child failed:', e);
		}
	}

	// ─── Drag & Drop (svelte-dnd-action) ────────────────────────────────────

	function handleChildrenConsider(e: CustomEvent<{ items: TropeNode[] }>) {
		dndChildren = e.detail.items;
	}

	async function handleChildrenFinalize(e: CustomEvent<{ items: TropeNode[] }>) {
		dndChildren = e.detail.items;
		// Check if a new item was dropped from outside
		for (const item of e.detail.items) {
			if (item.parent_id !== node.id) {
				try {
					await api.post(`/ontology/nodes/${item.id}/move`, { new_parent_id: node.id });
					onRefresh?.();
				} catch (err) {
					console.error('Move failed:', err);
				}
				return;
			}
		}
	}

	// Legacy native DnD kept as fallback for the node row itself
	function handleDragStart(e: DragEvent) {
		e.dataTransfer?.setData('text/plain', node.id);
		e.dataTransfer!.effectAllowed = 'move';
	}

	function handleDragOver(e: DragEvent) {
		e.preventDefault();
		e.dataTransfer!.dropEffect = 'move';
		dragOver = true;
	}

	function handleDragLeave() {
		dragOver = false;
	}

	async function handleDrop(e: DragEvent) {
		e.preventDefault();
		dragOver = false;
		const sourceId = e.dataTransfer?.getData('text/plain');
		if (!sourceId || sourceId === node.id) return;

		try {
			await api.post(`/ontology/nodes/${sourceId}/move`, {
				new_parent_id: node.id,
			});
			onRefresh?.();
		} catch (e) {
			console.error('Move failed:', e);
			alert('移动失败：可能造成循环引用');
		}
	}
</script>

<div
	class="select-none"
	style="padding-left: {depth * 20}px"
	role="treeitem"
	tabindex="0"
	aria-expanded={expanded}
	aria-selected={isSelected}
>
	<!-- Node Row -->
	<div
		role="group"
		aria-label="节点操作：{node.label}"
		class="group flex items-center gap-1 py-1 px-2 rounded-lg transition-all duration-100
			{isSelected ? 'bg-amber-500/10 ring-1 ring-amber-500/30' : 'hover:bg-ink-800/60'}
			{dragOver ? 'ring-2 ring-amber-400/50 bg-amber-500/5' : ''}"
		draggable="true"
		ondragstart={handleDragStart}
		ondragover={handleDragOver}
		ondragleave={handleDragLeave}
		ondrop={handleDrop}
	>
		<!-- Drag handle -->
		<div class="opacity-0 group-hover:opacity-40 cursor-grab shrink-0">
			<GripVertical class="w-3 h-3 text-ink-500" />
		</div>

		<!-- Expand toggle -->
		<button
			type="button"
			class="w-5 h-5 flex items-center justify-center shrink-0 rounded hover:bg-ink-700"
			onclick={() => expanded = !expanded}
			aria-label={expanded ? '收起子节点' : '展开子节点'}
			aria-expanded={expanded}
		>
			{#if hasChildren || !node.is_leaf}
				{#if expanded}
					<ChevronDown class="w-3.5 h-3.5 text-ink-400" />
				{:else}
					<ChevronRight class="w-3.5 h-3.5 text-ink-400" />
				{/if}
			{:else}
				<span class="w-1 h-1 rounded-full bg-ink-600"></span>
			{/if}
		</button>

		<!-- Label -->
		{#if editing}
			<input
				type="text"
				class="flex-1 bg-ink-800 border border-amber-500/50 rounded px-2 py-0.5 text-sm text-ink-100 focus:outline-none focus:ring-1 focus:ring-amber-500"
				bind:value={editLabel}
				onkeydown={(e) => { if (e.key === 'Enter') saveEdit(); if (e.key === 'Escape') editing = false; }}
				use:focusOnMount
			/>
		{:else}
			<button
				type="button"
				class="flex-1 flex items-center gap-2 text-left min-w-0"
				onclick={() => { selectedId = node.id; showActions = !showActions; }}
				ondblclick={startEdit}
			>
				<span class="text-sm text-ink-200 truncate">{node.label}</span>
			</button>
		{/if}

		<!-- Badges -->
		<div class="flex items-center gap-1 shrink-0">
			{#if node.cluster_size > 0}
				<span class="text-[10px] px-1.5 py-0.5 rounded-full bg-amber-500/10 text-amber-400">
					{node.cluster_size}
				</span>
			{/if}
		</div>

		<!-- Quick actions (visible on hover) -->
		<div class="hidden group-hover:flex items-center gap-0.5 shrink-0">
			<button
				type="button"
				class="p-1 rounded hover:bg-ink-700 text-ink-500 hover:text-amber-400"
				title="扫描全库"
				onclick={(e) => { e.stopPropagation(); scanNode(); }}
			>
				<Scan class="w-3.5 h-3.5 {scanning ? 'animate-spin' : ''}" />
			</button>
			<button
				type="button"
				class="p-1 rounded hover:bg-ink-700 text-ink-500 hover:text-green-400"
				title="进化子节点"
				onclick={(e) => { e.stopPropagation(); evolveNode(); }}
			>
				<Sparkles class="w-3.5 h-3.5 {evolving ? 'animate-pulse' : ''}" />
			</button>
			<button
				type="button"
				class="p-1 rounded hover:bg-ink-700 text-ink-500 hover:text-blue-400"
				title="添加子节点"
				onclick={(e) => { e.stopPropagation(); addingChild = true; expanded = true; }}
			>
				<Plus class="w-3.5 h-3.5" />
			</button>
			<button
				type="button"
				class="p-1 rounded hover:bg-ink-700 text-ink-500 hover:text-ink-200"
				title="编辑"
				onclick={(e) => { e.stopPropagation(); startEdit(); }}
			>
				<Pencil class="w-3 h-3" />
			</button>
			<button
				type="button"
				class="p-1 rounded hover:bg-ink-700 text-ink-500 hover:text-red-400"
				title="删除"
				onclick={(e) => { e.stopPropagation(); deleteNode(); }}
			>
				<Trash2 class="w-3 h-3" />
			</button>
		</div>
	</div>

	<!-- Expanded content -->
	{#if expanded}
		<!-- Add child inline input -->
		{#if addingChild}
			<div class="flex items-center gap-2 py-1" style="padding-left: {(depth + 1) * 20 + 28}px">
				<input
					type="text"
					class="flex-1 bg-ink-800 border border-ink-700 rounded px-2 py-1 text-sm text-ink-200 placeholder:text-ink-600 focus:border-amber-500/50 focus:outline-none"
					placeholder="新概念名称..."
					bind:value={newChildLabel}
					onkeydown={(e) => { if (e.key === 'Enter') addChild(); if (e.key === 'Escape') addingChild = false; }}
					use:focusOnMount
				/>
				<button
					type="button"
					class="px-2 py-1 text-xs bg-amber-600 hover:bg-amber-500 text-white rounded"
					onclick={addChild}
				>
					创建
				</button>
			</div>
		{/if}

		<!-- Children (with dnd-zone for reordering) -->
		{#if dndChildren.length > 0}
			<div
				use:dndzone={{ items: dndChildren, flipDurationMs: 200, type: 'tree-nodes' }}
				onconsider={handleChildrenConsider}
				onfinalize={handleChildrenFinalize}
			>
				{#each dndChildren as child (child.id)}
					<SettingNode
						node={child}
						{allNodes}
						depth={depth + 1}
						bind:selectedId
						{onRefresh}
					/>
				{/each}
			</div>
		{/if}

		<!-- Scan results (shown inline under the node) -->
		{#if showResults && scanResults.length > 0}
			<div class="ml-8 mt-1 mb-2 border-l-2 border-amber-500/20 pl-3 space-y-1.5" style="padding-left: 12px; margin-left: {(depth + 1) * 20 + 8}px">
				<div class="flex items-center gap-2 text-[10px] text-ink-500 py-1">
					<span>匹配结果 ({scanResults.length})</span>
					<button type="button" class="text-ink-400 hover:text-ink-200" onclick={() => showResults = false}>收起</button>
				</div>
				{#each scanResults.slice(0, 10) as chunk}
					<a
						class="block px-2 py-1.5 rounded bg-ink-900/60 border border-ink-800 hover:border-amber-500/30 transition-colors"
						href="/reading/{chunk.book_id}?chapter={chunk.chapter_index}&chunk={chunk.chunk_index}"
					>
						<div class="flex items-center gap-2 text-[10px] text-ink-500">
							<span>{langFlags[chunk.language ?? 'unknown'] ?? '🌐'}</span>
							<span class="truncate">{chunk.book_title}</span>
							<span>Ch.{chunk.chapter_index}</span>
							<span class="ml-auto text-amber-400/70">{(chunk.score * 100).toFixed(0)}%</span>
						</div>
						<p class="text-xs text-ink-300 line-clamp-2 mt-0.5">{chunk.text}</p>
					</a>
				{/each}
				{#if scanResults.length > 10}
					<p class="text-[10px] text-ink-600 pl-2">还有 {scanResults.length - 10} 条结果...</p>
				{/if}
			</div>
		{/if}

		<!-- Evolve proposals (accept/reject UI) -->
		{#if evolveProposals}
			<div style="padding-left: {(depth + 1) * 20 + 8}px" class="mt-2 mb-2">
				<EvolveProposals
					parentId={node.id}
					parentLabel={node.label}
					proposals={evolveProposals}
					evidenceChunks={evolveEvidence}
					onDone={handleEvolveDecisions}
				/>
			</div>
		{/if}
	{/if}
</div>
