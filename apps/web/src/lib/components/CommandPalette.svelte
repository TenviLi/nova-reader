<script lang="ts">
	import { goto } from '$app/navigation';
	import type { ComponentType } from 'svelte';
	import { api } from '$services/api';
	import {
		Library, BookOpen, Search, Network, User, PenTool,
		BarChart3, Globe, Edit, Cog, Folder, Pencil, RefreshCw, Compass,
		TreePine, MessageCircle, Radar, FileText
	} from 'lucide-svelte';

	let open = $state(false);
		let query = $state('');
		let selectedIndex = $state(0);
		let searchInput = $state<HTMLInputElement | null>(null);
		let bookResults = $state<Array<{ id: string; title: string; author: string }>>([]);
		let searchingBooks = $state(false);
		let recentReading = $state<Array<{ id: string; title: string; author: string }>>([]);

		$effect(() => {
			if (open) queueMicrotask(() => searchInput?.focus());
		});

	// Load recently reading books when palette opens
	$effect(() => {
		if (open && recentReading.length === 0) {
			api.getBooks({ reading_status: 'reading', sort_by: 'updated_at', per_page: 5 })
				.then(res => {
					recentReading = (res.data ?? []).map(b => ({ id: b.id, title: b.title, author: b.author ?? '未知' }));
				})
				.catch(() => {});
		}
	});

	type CommandItem = {
		id: string;
		label: string;
		description?: string;
		icon: ComponentType;
		action: () => void;
		category: string;
	};

	const commands: CommandItem[] = [
		{ id: 'nav-library', label: '所有书籍', description: '跨书库浏览和整理', icon: BookOpen, action: () => goto('/library'), category: '导航' },
		{ id: 'nav-libraries', label: '书库管理', description: '管理书库、扫描和权限', icon: Library, action: () => goto('/libraries'), category: '导航' },
		{ id: 'nav-series', label: '系列', description: '管理小说系列', icon: BookOpen, action: () => goto('/series'), category: '导航' },
		{ id: 'nav-search', label: '搜索', description: '全文 + 语义检索', icon: Search, action: () => goto('/search'), category: '导航' },
		{ id: 'nav-discover', label: '探索', description: '主题浏览 & 情绪推荐', icon: Compass, action: () => goto('/discover'), category: '导航' },
		{ id: 'nav-ontology', label: '设定花园', description: '本体论知识树 · 扫描 · 进化', icon: TreePine, action: () => goto('/ontology'), category: '知识' },
		{ id: 'nav-graph', label: '知识图谱', description: '实体关系可视化', icon: Network, action: () => goto('/graph'), category: '知识' },
		{ id: 'nav-characters', label: '人物志', description: '角色数据库', icon: User, action: () => goto('/characters'), category: '知识' },
		{ id: 'nav-semantic', label: '智能标签', description: '智能标签画像 & 热力图', icon: Radar, action: () => goto('/semantic-tags'), category: '知识' },
		{ id: 'nav-chat', label: 'AI 对话', description: '基于 RAG 的对话', icon: MessageCircle, action: () => goto('/chat'), category: 'AI' },
		{ id: 'nav-translate', label: '翻译', description: '术语感知 AI 翻译', icon: Globe, action: () => goto('/translate'), category: 'AI' },
		{ id: 'nav-analysis', label: '文本分析', description: '情感/伏笔/宏观结构', icon: FileText, action: () => goto('/analysis'), category: 'AI' },
		{ id: 'nav-writing', label: '创作工坊', description: 'AI 辅助创作', icon: Edit, action: () => goto('/writing'), category: 'AI' },
		{ id: 'nav-persons', label: '作者', description: '作者与译者管理', icon: PenTool, action: () => goto('/persons'), category: '管理' },
		{ id: 'nav-stats', label: '统计', description: '阅读热力图 & 目标', icon: BarChart3, action: () => goto('/stats'), category: '管理' },
		{ id: 'nav-collections', label: '书单', description: '书单与智能书架', icon: Folder, action: () => goto('/collections'), category: '管理' },
		{ id: 'nav-tasks', label: '任务队列', icon: Cog, action: () => goto('/tasks'), category: '管理' },
		{ id: 'nav-admin', label: '管理面板', icon: Cog, action: () => goto('/admin'), category: '管理' },
		{ id: 'action-batch', label: '批量编辑', description: '多选修改元数据', icon: Pencil, action: () => goto('/library/batch-edit'), category: '操作' },
		{ id: 'action-scan', label: '选择书库扫描', description: '在书库管理中选择书库扫描', icon: RefreshCw, action: () => goto('/libraries'), category: '操作' },
	];

	let filteredCommands = $derived(
		query.trim()
			? commands.filter(c => c.label.includes(query) || (c.description?.includes(query) ?? false))
			: commands
	);

	function handleKeydown(e: KeyboardEvent) {
		if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
			e.preventDefault();
			open = !open;
			return;
		}
		if (!open) return;

		if (e.key === 'Escape') {
			open = false;
		} else if (e.key === 'ArrowDown') {
			e.preventDefault();
			selectedIndex = Math.min(selectedIndex + 1, filteredCommands.length - 1);
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			selectedIndex = Math.max(selectedIndex - 1, 0);
		} else if (e.key === 'Enter') {
			e.preventDefault();
			const item = filteredCommands[selectedIndex];
			if (item) {
				item.action();
				open = false;
				query = '';
			}
		}
	}

	$effect(() => {
		if (query) selectedIndex = 0;
	});

	// Debounced book search when query doesn't match commands
	let searchTimeout: ReturnType<typeof setTimeout> | null = null;
	$effect(() => {
		if (searchTimeout) clearTimeout(searchTimeout);
		if (query.trim().length >= 2 && filteredCommands.length === 0) {
			searchingBooks = true;
			searchTimeout = setTimeout(async () => {
				try {
					const result = await api.getBooks({ search: query.trim(), per_page: 5 });
					bookResults = result.data.map(b => ({ id: b.id, title: b.title, author: b.author ?? '' }));
				} catch {
					bookResults = [];
				} finally {
					searchingBooks = false;
				}
			}, 300);
		} else {
			bookResults = [];
			searchingBooks = false;
		}
	});
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
	<!-- Backdrop -->
	<div role="presentation" class="fixed inset-0 z-[100] flex items-start justify-center bg-black/60 pt-[15vh] backdrop-blur-sm" onclick={() => open = false}>
		<!-- Modal -->
			<div
				role="dialog"
				aria-label="命令面板"
				tabindex="-1"
				class="w-full max-w-lg overflow-hidden rounded-2xl border border-ink-700/50 bg-ink-900 shadow-2xl animate-scale-in"
				onclick={(e) => e.stopPropagation()}
				onkeydown={(e) => e.stopPropagation()}
			>
			<!-- Search input -->
			<div class="flex items-center gap-3 border-b border-ink-800/50 px-4 py-3">
				<Search size={18} strokeWidth={2} class="text-ink-400 shrink-0" />
					<input
						type="text"
						bind:this={searchInput}
						bind:value={query}
						aria-label="输入命令或搜索"
						name="command-palette-search"
						autocomplete="off"
						placeholder="输入命令或搜索…"
						class="flex-1 bg-transparent text-sm text-ink-100 placeholder-ink-500 outline-none"
				/>
				<kbd class="rounded border border-ink-600 px-1.5 py-0.5 text-[10px] text-ink-500">ESC</kbd>
			</div>

			<!-- Results -->
			<div class="max-h-[50vh] overflow-y-auto p-2">
				{#if filteredCommands.length === 0 && bookResults.length === 0 && !searchingBooks}
					<div class="py-8 text-center text-sm text-ink-500">
						{query.trim().length >= 2 ? '没有匹配结果' : '没有匹配的命令'}
					</div>
				{:else if filteredCommands.length === 0 && searchingBooks}
					<div class="py-8 text-center text-sm text-ink-500">
						搜索中…
					</div>
				{:else if filteredCommands.length === 0 && bookResults.length > 0}
					<div class="mb-2">
						<div class="px-3 py-1.5 text-[10px] font-medium uppercase tracking-wider text-ink-500">
							书籍
						</div>
						{#each bookResults as book}
							<button
								onclick={() => { goto(`/reading/${book.id}`); open = false; query = ''; }}
								class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left text-sm transition-colors hover:bg-ink-800"
							>
								<BookOpen size={16} strokeWidth={2} class="text-accent-400 shrink-0" />
								<div class="flex-1 min-w-0">
									<span class="text-ink-100 truncate block">{book.title}</span>
									<span class="text-xs text-ink-500">{book.author}</span>
								</div>
							</button>
						{/each}
					</div>
				{:else}
					<!-- Recently reading (shown above commands when no query) -->
					{#if !query.trim() && recentReading.length > 0}
						<div class="mb-2">
							<div class="px-3 py-1.5 text-[10px] font-medium uppercase tracking-wider text-ink-500">
								最近阅读
							</div>
							{#each recentReading as book}
								<button
									onclick={() => { goto(`/reading/${book.id}`); open = false; query = ''; }}
									class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left text-sm transition-colors hover:bg-ink-800"
								>
									<BookOpen size={14} strokeWidth={2} class="text-accent-400 shrink-0" />
									<span class="text-ink-200 truncate">{book.title}</span>
									<span class="text-xs text-ink-500 ml-auto shrink-0">{book.author}</span>
								</button>
							{/each}
						</div>
					{/if}
					{@const grouped = Object.groupBy(filteredCommands, c => c.category)}
					{#each Object.entries(grouped) as [category, items], ci}
						<div class="mb-2">
							<div class="px-3 py-1.5 text-[10px] font-medium uppercase tracking-wider text-ink-500">
								{category}
							</div>
							{#each items ?? [] as item, i}
								{@const globalIndex = filteredCommands.indexOf(item)}
								{@const ItemIcon = item.icon}
								<button
									onclick={() => { item.action(); open = false; query = ''; }}
									onmouseenter={() => selectedIndex = globalIndex}
									class="flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left transition-colors {selectedIndex === globalIndex ? 'bg-accent-500/10' : ''}"
									class:text-accent-400={selectedIndex === globalIndex}
								>
									<div class="flex h-8 w-8 items-center justify-center rounded-lg bg-ink-800/80">
										<ItemIcon size={15} strokeWidth={1.8} class="text-ink-300" />
									</div>
									<div class="flex-1">
										<div class="text-sm text-ink-200">{item.label}</div>
										{#if item.description}
											<div class="text-xs text-ink-500">{item.description}</div>
										{/if}
									</div>
								</button>
							{/each}
						</div>
					{/each}
				{/if}
			</div>
		</div>
	</div>
{/if}

<style>
	@keyframes scale-in {
		from { transform: scale(0.95); opacity: 0; }
		to { transform: scale(1); opacity: 1; }
	}
	.animate-scale-in {
		animation: scale-in 0.15s ease-out;
	}
</style>
