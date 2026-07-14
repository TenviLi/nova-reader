<script lang="ts">
	import { api } from '$services/api';
	import { onMount } from 'svelte';
	import {
		TreePine, Plus, RefreshCw, Sparkles, Scan,
		Layers, Globe, BookOpen, Zap, Users, GitBranch, ArrowRight, CheckSquare, Trash2
	} from 'lucide-svelte';
	import SettingNode from '$lib/components/ontology/SettingNode.svelte';
	import { dndzone } from 'svelte-dnd-action';

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

	interface OntologyTree {
		nodes: TropeNode[];
		total_chunks: number;
		max_depth: number;
	}

	interface DriftTimeline {
		character_name: string;
		book_id: string;
		snapshots: PersonaSnapshot[];
		events: DriftEvent[];
		total_drift: number;
	}

	interface PersonaSnapshot {
		id: string;
		book_id: string;
		character_name: string;
		chapter_index: number;
		dialogue_count: number;
		monologue_count: number;
		drift_from_prev: number | null;
		drift_from_baseline: number | null;
		computed_at: string;
	}

	interface DriftEvent {
		id: string;
		book_id: string;
		character_name: string;
		chapter_index: number;
		drift_magnitude: number;
		drift_direction: string | null;
		evidence_text: string | null;
		target_persona: string | null;
		event_type: string;
	}

	interface SettingRule {
		id: string;
		book_id: string;
		trope_node_id: string | null;
		subject_type: string;
		subject_label: string;
		predicate: string;
		object_type: string;
		object_label: string;
		properties: Record<string, unknown>;
		constraints: string[] | null;
		source_text: string | null;
		confidence: number;
	}

	// ─── State ──────────────────────────────────────────────────────────────

	let activeTab = $state<'garden' | 'persona' | 'rules'>('garden');
	let tree = $state<OntologyTree | null>(null);
	let loading = $state(false);
	let selectedId = $state('');
	let domainFilter = $state('');
	let addingRoot = $state(false);
	let newRootLabel = $state('');
	let newRootDomain = $state('general');
	let newRootDesc = $state('');

	// Batch operations
	let batchMode = $state(false);
	let batchSelected = $state<Set<string>>(new Set());

	// Persona drift
	let trackedCharacters = $state<{ character_name: string; chapters_tracked: number; max_drift_from_baseline: number | null }[]>([]);
	let selectedTimeline = $state<DriftTimeline | null>(null);
	let personaBookId = $state<string>('');
	let trackCharName = $state<string>('');

	// Rule splicing
	let bookRules = $state<SettingRule[]>([]);
	let selectedRules = $state<string[]>([]);
	let spliceResult = $state<{ narrative: string; conflicts: string[] } | null>(null);
	let rulesBookId = $state<string>('');

	// ─── Data Fetching ──────────────────────────────────────────────────────

	async function loadTree() {
		loading = true;
		try {
			const params = domainFilter ? `?domain=${domainFilter}` : '';
			tree = await api.get(`/ontology/tree${params}`);
		} catch (e) {
			console.error('Failed to load tree:', e);
		} finally {
			loading = false;
		}
	}

	async function createRootNode() {
		if (!newRootLabel.trim()) return;
		try {
			await api.post('/ontology/nodes', {
				label: newRootLabel.trim(),
				description: newRootDesc.trim() || undefined,
				domain: newRootDomain,
				parent_id: null,
			});
			newRootLabel = '';
			newRootDesc = '';
			addingRoot = false;
			await loadTree();
		} catch (e) {
			console.error('Create failed:', e);
		}
	}

	async function scanAll() {
		if (!tree) return;
		const roots = tree.nodes.filter(n => n.level === 0);
		for (const root of roots) {
			try {
				await api.post(`/ontology/nodes/${root.id}/scan`, { limit: 50 });
			} catch (e) {
				console.error(`Scan failed for ${root.label}:`, e);
			}
		}
		await loadTree();
	}

	// Persona drift functions
	async function loadCharacters() {
		if (!personaBookId) return;
		try {
			trackedCharacters = await api.get(`/persona/book/${personaBookId}/characters`);
		} catch (e) {
			console.error('Failed to load characters:', e);
		}
	}

	async function trackCharacter() {
		if (!personaBookId || !trackCharName) return;
		try {
			await api.post('/persona/track', { book_id: personaBookId, character_name: trackCharName });
			await loadCharacters();
			trackCharName = '';
		} catch (e) {
			console.error('Failed to track persona:', e);
		}
	}

	async function loadTimeline(characterName: string) {
		if (!personaBookId) return;
		try {
			selectedTimeline = await api.get(`/persona/book/${personaBookId}/${encodeURIComponent(characterName)}/timeline`);
		} catch (e) {
			console.error('Failed to load timeline:', e);
		}
	}

	// Rule splicing functions
	async function loadBookRules() {
		if (!rulesBookId) return;
		try {
			bookRules = await api.get(`/rules/book/${rulesBookId}`);
		} catch (e) {
			console.error('Failed to load rules:', e);
		}
	}

	async function extractRules() {
		if (!rulesBookId) return;
		try {
			await api.post('/rules/extract', { book_id: rulesBookId, domain: domainFilter || undefined });
			await loadBookRules();
		} catch (e) {
			console.error('Rule extraction failed:', e);
		}
	}

	async function spliceSelected() {
		if (selectedRules.length < 2) return;
		try {
			spliceResult = await api.post('/rules/splice', { rule_ids: selectedRules });
		} catch (e) {
			console.error('Splice failed:', e);
		}
	}

	// ─── Batch Operations ───────────────────────────────────────────────────

	function toggleBatchSelect(id: string) {
		const next = new Set(batchSelected);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		batchSelected = next;
	}

	async function batchDelete() {
		if (batchSelected.size === 0) return;
		if (!confirm(`确定删除选中的 ${batchSelected.size} 个节点？`)) return;
		for (const id of batchSelected) {
			try {
				await api.del(`/ontology/nodes/${id}`);
			} catch (e) {
				console.error(`Delete ${id} failed:`, e);
			}
		}
		batchSelected = new Set();
		batchMode = false;
		await loadTree();
	}

	async function batchMove(targetParentId: string | null) {
		if (batchSelected.size === 0) return;
		for (const id of batchSelected) {
			try {
				await api.post(`/ontology/nodes/${id}/move`, { new_parent_id: targetParentId });
			} catch (e) {
				console.error(`Move ${id} failed:`, e);
			}
		}
		batchSelected = new Set();
		batchMode = false;
		await loadTree();
	}

	async function batchScan() {
		if (batchSelected.size === 0) return;
		for (const id of batchSelected) {
			try {
				await api.post(`/ontology/nodes/${id}/scan`, { limit: 30, threshold: 0.4 });
			} catch (e) {
				console.error(`Scan ${id} failed:`, e);
			}
		}
		await loadTree();
	}

	async function batchChangeDomain(newDomain: string) {
		if (batchSelected.size === 0) return;
		for (const id of batchSelected) {
			try {
				await api.put(`/ontology/nodes/${id}`, { domain: newDomain });
			} catch (e) {
				console.error(`Update ${id} domain failed:`, e);
			}
		}
		batchSelected = new Set();
		await loadTree();
	}

	// DnD zone handler for root-level reorder
	function handleDndConsider(e: CustomEvent<{ items: TropeNode[] }>) {
		dndRootItems = e.detail.items;
	}

	function handleDndFinalize(e: CustomEvent<{ items: TropeNode[] }>) {
		dndRootItems = e.detail.items;
		// Persist new order (future: add sort_order field)
	}

	onMount(loadTree);

	// ─── Derived ────────────────────────────────────────────────────────────

	let rootNodes = $derived(tree?.nodes.filter(n => n.level === 0) ?? []);
	let dndRootItems = $state<TropeNode[]>([]);
	$effect(() => { dndRootItems = [...rootNodes]; });
	let selectedNode = $derived(tree?.nodes.find(n => n.id === selectedId) ?? null);

	const domains = [
		{ value: '', label: '全部' },
		{ value: 'worldbuilding', label: '世界观' },
		{ value: 'power_system', label: '力量体系' },
		{ value: 'relationship', label: '人物关系' },
		{ value: 'trope', label: '桥段母题' },
		{ value: 'tone', label: '氛围调性' },
		{ value: 'general', label: '通用' },
	];

	const eventTypeLabels: Record<string, string> = {
		drift: '人格漂移',
		fusion: '人格融合',
		reversion: '人格回归',
		awakening: '意识觉醒',
	};
</script>

<svelte:head>
	<title>设定花园 | Nova Reader</title>
</svelte:head>

<div class="min-h-screen bg-ink-950 text-ink-100">
	<!-- Header -->
	<div class="border-b border-ink-800 bg-ink-900/50 backdrop-blur sticky top-0 z-10">
		<div class="max-w-7xl mx-auto px-6 py-4">
			<div class="flex items-center justify-between">
				<div class="flex items-center gap-3">
					<TreePine class="w-6 h-6 text-amber-400" />
					<div>
						<h1 class="text-lg font-bold">设定花园</h1>
						<p class="text-xs text-ink-500">
							自由创建 → 全库扫描 → AI进化 · 多语言自动匹配
						</p>
					</div>
				</div>
				<div class="flex items-center gap-2">
					{#if tree}
						<span class="text-xs text-ink-500">
							{tree.nodes.length} 概念 · {tree.total_chunks} 语料
						</span>
					{/if}
				</div>
			</div>
		</div>
	</div>

	<div class="max-w-7xl mx-auto px-6 py-4">
		<!-- Tab Navigation -->
		<div class="flex gap-1 mb-4 bg-ink-900 rounded-lg p-1 w-fit">
			<button
				class="px-4 py-2 rounded-md text-sm font-medium transition-colors
					{activeTab === 'garden' ? 'bg-amber-500/20 text-amber-400' : 'text-ink-400 hover:text-ink-200'}"
				onclick={() => activeTab = 'garden'}
			>
				<TreePine class="w-4 h-4 inline mr-1" />
				设定花园
			</button>
			<button
				class="px-4 py-2 rounded-md text-sm font-medium transition-colors
					{activeTab === 'persona' ? 'bg-amber-500/20 text-amber-400' : 'text-ink-400 hover:text-ink-200'}"
				onclick={() => activeTab = 'persona'}
			>
				<Users class="w-4 h-4 inline mr-1" />
				人格漂移
			</button>
			<button
				class="px-4 py-2 rounded-md text-sm font-medium transition-colors
					{activeTab === 'rules' ? 'bg-amber-500/20 text-amber-400' : 'text-ink-400 hover:text-ink-200'}"
				onclick={() => activeTab = 'rules'}
			>
				<GitBranch class="w-4 h-4 inline mr-1" />
				规则缝合
			</button>
		</div>

		<!-- ═══════════ Garden Tab ═══════════ -->
		{#if activeTab === 'garden'}
			<!-- Toolbar -->
			<div class="flex items-center gap-3 mb-4 flex-wrap">
				<div class="flex gap-1 bg-ink-900 rounded-lg p-1">
					{#each domains as d}
						<button
							class="px-3 py-1.5 rounded-md text-xs font-medium transition-colors
								{domainFilter === d.value ? 'bg-amber-500/20 text-amber-400' : 'text-ink-400 hover:text-ink-200 hover:bg-ink-800'}"
							onclick={() => { domainFilter = d.value; loadTree(); }}
						>
							{d.label}
						</button>
					{/each}
				</div>

				<div class="flex items-center gap-2 ml-auto">
					<button
						class="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-colors
							{batchMode ? 'bg-blue-500/20 text-blue-400 ring-1 ring-blue-500/30' : 'bg-ink-800 hover:bg-ink-700 text-ink-200'}"
						onclick={() => { batchMode = !batchMode; if (!batchMode) batchSelected = new Set(); }}
						title="批量操作模式"
					>
						<CheckSquare class="w-3.5 h-3.5" />
						批量
					</button>
					<button
						class="flex items-center gap-1.5 px-3 py-1.5 bg-amber-600 hover:bg-amber-500 text-white rounded-md text-xs font-medium"
						onclick={() => addingRoot = true}
					>
						<Plus class="w-3.5 h-3.5" />
						新建根概念
					</button>
					<button
						class="flex items-center gap-1.5 px-3 py-1.5 bg-ink-800 hover:bg-ink-700 text-ink-200 rounded-md text-xs"
						onclick={scanAll}
						title="扫描全部根节点"
					>
						<Scan class="w-3.5 h-3.5" />
						全量扫描
					</button>
					<button
						class="flex items-center gap-1.5 px-3 py-1.5 bg-ink-800 hover:bg-ink-700 text-ink-200 rounded-md text-xs"
						onclick={loadTree}
					>
						<RefreshCw class="w-3.5 h-3.5" />
					</button>
				</div>
			</div>

			<!-- Batch action bar -->
			{#if batchMode && batchSelected.size > 0}
				<div class="mb-4 p-3 bg-blue-500/5 border border-blue-500/20 rounded-xl flex items-center gap-3 flex-wrap">
					<span class="text-xs text-blue-300 font-medium">
						已选 {batchSelected.size} 个节点
					</span>
					<div class="flex items-center gap-2">
						<button
							class="px-3 py-1 rounded-md text-xs bg-ink-800 hover:bg-ink-700 text-ink-200 transition-colors"
							onclick={batchScan}
						>
							<Scan class="w-3 h-3 inline mr-1" />
							批量扫描
						</button>
						<select
							class="px-3 py-1 rounded-md text-xs bg-ink-800 border border-ink-700 text-ink-200"
							onchange={(e) => {
								const val = (e.target as HTMLSelectElement).value;
								if (val) batchChangeDomain(val);
							}}
						>
							<option value="">修改领域...</option>
							{#each domains.filter(d => d.value) as d}
								<option value={d.value}>{d.label}</option>
							{/each}
						</select>
						<button
							class="px-3 py-1 rounded-md text-xs bg-red-900/50 hover:bg-red-800/60 text-red-300 transition-colors"
							onclick={batchDelete}
						>
							<Trash2 class="w-3 h-3 inline mr-1" />
							删除
						</button>
					</div>
					<button
						class="ml-auto text-xs text-ink-500 hover:text-ink-300"
						onclick={() => batchSelected = new Set()}
					>
						清除选择
					</button>
				</div>
			{/if}

			<!-- Add root node form -->
			{#if addingRoot}
				<div class="mb-4 p-4 bg-ink-900 border border-amber-500/20 rounded-xl">
					<h3 class="text-sm font-medium text-amber-400 mb-3">创建新的根设定概念</h3>
					<div class="grid grid-cols-1 md:grid-cols-3 gap-3">
						<input
							type="text"
							class="bg-ink-800 border border-ink-700 rounded-md px-3 py-2 text-sm text-ink-200 placeholder:text-ink-600 focus:border-amber-500/50 focus:outline-none"
							placeholder="概念名称 (如: 修炼体系、宗门政治、宿命轮回)"
							bind:value={newRootLabel}
							onkeydown={(e) => { if (e.key === 'Enter') createRootNode(); }}
						/>
						<input
							type="text"
							class="bg-ink-800 border border-ink-700 rounded-md px-3 py-2 text-sm text-ink-200 placeholder:text-ink-600 focus:border-amber-500/50 focus:outline-none"
							placeholder="描述 (可选，帮助AI理解概念)"
							bind:value={newRootDesc}
						/>
						<select
							class="bg-ink-800 border border-ink-700 rounded-md px-3 py-2 text-sm text-ink-200"
							bind:value={newRootDomain}
						>
							{#each domains.filter(d => d.value) as d}
								<option value={d.value}>{d.label}</option>
							{/each}
						</select>
					</div>
					<div class="flex gap-2 mt-3">
						<button
							class="px-4 py-2 bg-amber-600 hover:bg-amber-500 text-white rounded-md text-sm font-medium"
							onclick={createRootNode}
						>
							创建
						</button>
						<button
							class="px-4 py-2 text-ink-400 hover:text-ink-200 text-sm"
							onclick={() => addingRoot = false}
						>
							取消
						</button>
					</div>
					<p class="text-xs text-ink-500 mt-2">
						创建后系统自动计算语义向量。然后你可以"扫描"全库找到匹配段落，再"进化"让AI发现子概念。
					</p>
				</div>
			{/if}

			<!-- Main Content: Tree + Detail Panel -->
			<div class="grid grid-cols-1 lg:grid-cols-4 gap-4">
				<!-- Tree Panel -->
				<div class="lg:col-span-3 bg-ink-900/50 rounded-xl border border-ink-800 overflow-hidden">
					<div class="p-3 border-b border-ink-800 flex items-center justify-between">
						<span class="text-xs text-ink-400">
							拖拽移动 · 双击编辑 · 悬停按钮操作
						</span>
						<div class="flex items-center gap-1 text-[10px] text-ink-500">
							<Globe class="w-3 h-3" />
							中/日/英/韩 自动匹配
						</div>
					</div>

					<div class="p-2 max-h-[calc(100vh-280px)] overflow-y-auto" role="tree">
						{#if loading}
							<div class="flex justify-center py-12">
								<RefreshCw class="w-5 h-5 text-ink-500 animate-spin" />
							</div>
						{:else if dndRootItems.length > 0}
							<div
								use:dndzone={{ items: dndRootItems, flipDurationMs: 200, type: 'root-nodes' }}
								onconsider={handleDndConsider}
								onfinalize={handleDndFinalize}
							>
								{#each dndRootItems as node (node.id)}
									<div class="flex items-start gap-1">
										{#if batchMode}
											<button
												class="mt-1.5 p-0.5 rounded shrink-0 transition-colors
													{batchSelected.has(node.id) ? 'text-blue-400' : 'text-ink-600 hover:text-ink-400'}"
												onclick={() => toggleBatchSelect(node.id)}
											>
												<CheckSquare class="w-4 h-4" />
											</button>
										{/if}
										<div class="flex-1">
											<SettingNode
												{node}
												allNodes={tree?.nodes ?? []}
												depth={0}
												bind:selectedId
												onRefresh={loadTree}
											/>
										</div>
									</div>
								{/each}
							</div>
						{:else}
							<div class="text-center py-16">
								<TreePine class="w-16 h-16 mx-auto mb-4 text-ink-800" />
								<h3 class="text-ink-400 font-medium mb-2">设定花园是空的</h3>
								<p class="text-xs text-ink-500 max-w-sm mx-auto mb-4">
									从你最关心的设定概念开始。比如创建"修炼体系"、"宗门政治"、"宿命轮回"作为根节点，
									然后扫描你的小说库，让AI自动发现子类型。
								</p>
								<button
									class="px-4 py-2 bg-amber-600 hover:bg-amber-500 text-white rounded-md text-sm font-medium"
									onclick={() => addingRoot = true}
								>
									<Plus class="w-4 h-4 inline mr-1" />
									创建第一个概念
								</button>
							</div>
						{/if}
					</div>
				</div>

				<!-- Detail / Inspector Panel -->
				<div class="bg-ink-900/50 rounded-xl border border-ink-800 p-4 h-fit sticky top-20">
					{#if selectedNode}
						<div class="space-y-4">
							<div>
								<h3 class="font-medium text-amber-400 text-sm">{selectedNode.label}</h3>
								{#if selectedNode.description}
									<p class="text-xs text-ink-400 mt-1">{selectedNode.description}</p>
								{:else}
									<p class="text-xs text-ink-600 mt-1 italic">无描述</p>
								{/if}
							</div>

							<!-- Quick actions from inspector -->
							<div class="flex items-center gap-2">
								<button
									class="flex-1 px-2 py-1.5 text-[10px] font-medium rounded bg-amber-500/10 text-amber-400 hover:bg-amber-500/20 transition-colors"
									onclick={() => {
										// Trigger scan on the selected node via API
										api.post(`/ontology/nodes/${selectedNode.id}/scan`, { limit: 30, threshold: 0.4 })
											.then(() => loadTree());
									}}
								>
									<Scan class="w-3 h-3 inline mr-0.5" />
									扫描
								</button>
								<button
									class="flex-1 px-2 py-1.5 text-[10px] font-medium rounded bg-green-500/10 text-green-400 hover:bg-green-500/20 transition-colors"
									onclick={() => {
										api.post(`/ontology/nodes/${selectedNode.id}/evolve`, { max_children: 5, min_evidence: 3 })
											.then(() => loadTree());
									}}
								>
									<Sparkles class="w-3 h-3 inline mr-0.5" />
									进化
								</button>
							</div>

							<div class="grid grid-cols-2 gap-2 text-xs">
								<div class="bg-ink-800 rounded-lg p-2">
									<div class="text-ink-500">匹配语料</div>
									<div class="text-ink-200 font-medium">{selectedNode.cluster_size}</div>
								</div>
								<div class="bg-ink-800 rounded-lg p-2">
									<div class="text-ink-500">层级</div>
									<div class="text-ink-200 font-medium">L{selectedNode.level}</div>
								</div>
								<div class="bg-ink-800 rounded-lg p-2">
									<div class="text-ink-500">领域</div>
									<div class="text-ink-200 font-medium">{selectedNode.domain}</div>
								</div>
								<div class="bg-ink-800 rounded-lg p-2">
									<div class="text-ink-500">稳定度</div>
									<div class="text-ink-200 font-medium">{selectedNode.stability.toFixed(2)}</div>
								</div>
							</div>

							{#if selectedNode.attributes && Object.keys(selectedNode.attributes).length > 0}
								<div>
									<h4 class="text-[10px] text-ink-500 uppercase tracking-wider mb-1">提取属性</h4>
									<div class="space-y-1">
										{#each Object.entries(selectedNode.attributes) as [key, val]}
											<div class="flex justify-between text-xs bg-ink-800 rounded px-2 py-1">
												<span class="text-ink-400">{key}</span>
												<span class="text-amber-400 truncate ml-2">{JSON.stringify(val)}</span>
											</div>
										{/each}
									</div>
								</div>
							{/if}
						</div>
					{:else}
						<div class="text-center py-8">
							<Layers class="w-8 h-8 mx-auto text-ink-700 mb-2" />
							<p class="text-xs text-ink-500">选择一个节点查看详情</p>
						</div>
					{/if}

					<!-- Help -->
					<div class="mt-6 pt-4 border-t border-ink-800">
						<h4 class="text-[10px] text-ink-500 uppercase tracking-wider mb-2">使用流程</h4>
						<ol class="text-[10px] text-ink-500 space-y-1 list-decimal list-inside">
							<li>创建概念节点（如"修炼体系"）</li>
							<li>点扫描按钮匹配全库段落</li>
							<li>点进化按钮让AI发现子概念</li>
							<li>重复：扫描 → 进化 → 树自动生长</li>
							<li>拖拽调整层级结构</li>
						</ol>
					</div>
				</div>
			</div>

		<!-- ═══════════ Persona Tab ═══════════ -->
		{:else if activeTab === 'persona'}
			<div class="space-y-4">
				<div class="flex items-center gap-3 flex-wrap">
					<input
						type="text"
						class="bg-ink-800 border border-ink-700 rounded-md px-3 py-2 text-sm text-ink-200 w-64"
						placeholder="书籍 ID"
						bind:value={personaBookId}
						onchange={() => loadCharacters()}
					/>
					<input
						type="text"
						class="bg-ink-800 border border-ink-700 rounded-md px-3 py-2 text-sm text-ink-200 w-48"
						placeholder="角色名"
						bind:value={trackCharName}
					/>
					<button
						class="px-4 py-2 bg-amber-600 hover:bg-amber-500 text-white rounded-md text-sm font-medium"
						onclick={trackCharacter}
					>
						追踪人格
					</button>
				</div>

				{#if trackedCharacters.length > 0}
					<div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
						{#each trackedCharacters as char}
							<button
								class="bg-ink-900 border border-ink-800 rounded-lg p-4 text-left hover:border-amber-500/30 transition-colors"
								onclick={() => loadTimeline(char.character_name)}
							>
								<div class="flex items-center gap-2 mb-2">
									<Users class="w-4 h-4 text-amber-400" />
									<span class="font-medium text-sm">{char.character_name}</span>
								</div>
								<div class="text-xs text-ink-400 space-y-1">
									<div>追踪章节: {char.chapters_tracked}</div>
									{#if char.max_drift_from_baseline != null}
										<div class="flex items-center gap-2">
											<span>最大偏移:</span>
											<div class="flex-1 bg-ink-800 rounded-full h-1.5">
												<div
													class="h-full rounded-full transition-all"
													class:bg-green-500={char.max_drift_from_baseline < 0.15}
													class:bg-yellow-500={char.max_drift_from_baseline >= 0.15 && char.max_drift_from_baseline < 0.3}
													class:bg-red-500={char.max_drift_from_baseline >= 0.3}
													style="width: {Math.min(char.max_drift_from_baseline * 100, 100)}%"
												></div>
											</div>
											<span class="text-ink-300">{(char.max_drift_from_baseline * 100).toFixed(1)}%</span>
										</div>
									{/if}
								</div>
							</button>
						{/each}
					</div>
				{/if}

				{#if selectedTimeline}
					<div class="bg-ink-900 rounded-xl border border-ink-800 p-6">
						<div class="flex items-center gap-2 mb-4">
							<h3 class="font-medium text-amber-400">{selectedTimeline.character_name}</h3>
							<span class="text-xs text-ink-500">
								总漂移: {(selectedTimeline.total_drift * 100).toFixed(1)}%
							</span>
						</div>

						<div class="mb-6">
							<h4 class="text-xs text-ink-400 mb-2">章节向量漂移曲线</h4>
							<div class="flex items-end gap-0.5 h-32">
								{#each selectedTimeline.snapshots as snap}
									{@const height = (snap.drift_from_baseline ?? 0) * 100}
									{@const barColor = height < 15 ? 'bg-green-500/60' : height < 30 ? 'bg-yellow-500/60' : 'bg-red-500/60'}
									<div
										class="flex-1 rounded-t transition-all relative group {barColor}"
										style="height: {Math.max(height, 2)}%"
									>
										<div class="absolute bottom-full left-1/2 -translate-x-1/2 mb-1 hidden group-hover:block bg-ink-800 text-ink-200 text-xs px-2 py-1 rounded whitespace-nowrap z-10">
											Ch.{snap.chapter_index}: {((snap.drift_from_baseline ?? 0) * 100).toFixed(1)}%
										</div>
									</div>
								{/each}
							</div>
							<div class="flex justify-between text-xs text-ink-500 mt-1">
								<span>Ch.{selectedTimeline.snapshots[0]?.chapter_index ?? '?'}</span>
								<span>Ch.{selectedTimeline.snapshots[selectedTimeline.snapshots.length - 1]?.chapter_index ?? '?'}</span>
							</div>
						</div>

						{#if selectedTimeline.events.length > 0}
							<h4 class="text-xs text-ink-400 mb-2">检测到的漂移事件</h4>
							<div class="space-y-2">
								{#each selectedTimeline.events as event}
									<div class="flex items-start gap-3 bg-ink-800/50 rounded-lg p-3">
										<Zap class="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
										<div class="flex-1 min-w-0">
											<div class="flex items-center gap-2 text-xs">
												<span class="font-medium text-ink-200">第 {event.chapter_index} 章</span>
												<span class="px-1.5 py-0.5 rounded text-[10px] bg-amber-500/20 text-amber-400">
													{eventTypeLabels[event.event_type] ?? event.event_type}
												</span>
												<span class="text-ink-500">强度 {(event.drift_magnitude * 100).toFixed(1)}%</span>
											</div>
											{#if event.evidence_text}
												<p class="text-xs text-ink-400 mt-1 line-clamp-2">{event.evidence_text}</p>
											{/if}
										</div>
									</div>
								{/each}
							</div>
						{/if}
					</div>
				{/if}
			</div>

		<!-- ═══════════ Rules Tab ═══════════ -->
		{:else if activeTab === 'rules'}
			<div class="space-y-4">
				<div class="flex items-center gap-3 flex-wrap">
					<input
						type="text"
						class="bg-ink-800 border border-ink-700 rounded-md px-3 py-2 text-sm text-ink-200 w-64"
						placeholder="书籍 ID"
						bind:value={rulesBookId}
						onchange={() => loadBookRules()}
					/>
					<button
						class="px-4 py-2 bg-amber-600 hover:bg-amber-500 text-white rounded-md text-sm font-medium"
						onclick={extractRules}
					>
						<Sparkles class="w-4 h-4 inline mr-1" />
						提取规则
					</button>
					<button
						class="px-4 py-2 bg-ink-700 hover:bg-ink-600 text-ink-200 rounded-md text-sm disabled:opacity-50"
						onclick={spliceSelected}
						disabled={selectedRules.length < 2}
					>
						<GitBranch class="w-4 h-4 inline mr-1" />
						缝合选中 ({selectedRules.length})
					</button>
				</div>

				{#if bookRules.length > 0}
					<div class="bg-ink-900 rounded-xl border border-ink-800 divide-y divide-ink-800">
						{#each bookRules as rule}
							<label class="flex items-start gap-3 p-4 hover:bg-ink-800/30 cursor-pointer">
								<input
									type="checkbox"
									class="mt-1 rounded border-ink-600 bg-ink-800 text-amber-500 focus:ring-amber-500/30"
									checked={selectedRules.includes(rule.id)}
									onchange={() => {
										if (selectedRules.includes(rule.id)) {
											selectedRules = selectedRules.filter(id => id !== rule.id);
										} else {
											selectedRules = [...selectedRules, rule.id];
										}
									}}
								/>
								<div class="flex-1 min-w-0">
									<div class="flex items-center gap-2 text-sm">
										<span class="text-ink-200 font-medium">{rule.subject_label}</span>
										<ArrowRight class="w-3 h-3 text-ink-500" />
										<span class="text-amber-400">{rule.predicate}</span>
										<ArrowRight class="w-3 h-3 text-ink-500" />
										<span class="text-ink-200">{rule.object_label}</span>
									</div>
									{#if rule.source_text}
										<p class="text-xs text-ink-400 mt-1 line-clamp-2">{rule.source_text}</p>
									{/if}
									<div class="flex items-center gap-3 mt-1 text-xs text-ink-500">
										<span>置信度: {(rule.confidence * 100).toFixed(0)}%</span>
									</div>
								</div>
							</label>
						{/each}
					</div>
				{:else if rulesBookId}
					<div class="text-center py-8 text-ink-500">
						<BookOpen class="w-8 h-8 mx-auto mb-2 opacity-30" />
						<p class="text-sm">暂无提取的规则。点击"提取规则"开始。</p>
					</div>
				{/if}

				{#if spliceResult}
					<div class="bg-ink-900 rounded-xl border border-amber-500/30 p-6">
						<h3 class="font-medium text-amber-400 mb-3 flex items-center gap-2">
							<GitBranch class="w-4 h-4" />
							缝合结果
						</h3>
						<p class="text-sm text-ink-200 whitespace-pre-wrap mb-4">{spliceResult.narrative}</p>
						{#if spliceResult.conflicts.length > 0}
							<div class="border-t border-ink-800 pt-3">
								<h4 class="text-xs font-medium text-red-400 mb-2">检测到冲突</h4>
								<ul class="space-y-1">
									{#each spliceResult.conflicts as conflict}
										<li class="text-xs text-ink-400">⚠ {conflict}</li>
									{/each}
								</ul>
							</div>
						{/if}
					</div>
				{/if}
			</div>
		{/if}
	</div>
</div>
