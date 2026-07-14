<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { auth } from '$stores/auth.svelte';
	import { api } from '$services/api';
	import { onMount } from 'svelte';
	import { toast } from 'svelte-sonner';
	import {
		LayoutDashboard, Library, Layers, FolderOpen, BookOpen,
		Star, BarChart3, Search, PenTool, Languages,
		Sparkles, ListChecks, BookCopy, Radar, Network,
		ChevronsLeft, BookMarked, Compass, Plus, RefreshCw,
		MoreHorizontal, Pencil, Trash2, Cpu, Shield,
		MessageCircle, TreePine, Users, FileText
	} from 'lucide-svelte';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
	import LibraryDialog from './LibraryDialog.svelte';
	import type { ComponentType } from 'svelte';

	let { collapsed = $bindable(false) } = $props();

	interface NavItem { href: string; label: string; icon: ComponentType; adminOnly?: boolean }
	interface NavGroup { label: string; items: NavItem[]; adminOnly?: boolean }

	interface LibraryItem {
		id: string;
		name: string;
		book_count: number;
		root_path: string;
		description?: string | null;
	}

	let libraries = $state<LibraryItem[]>([]);
	let recentReading = $state<Array<{id: string; title: string; progress: number; cover_path?: string | null}>>([]);
	let dialogOpen = $state(false);
	let dialogMode = $state<'create' | 'edit'>('create');
	let editingLibrary = $state<LibraryItem | undefined>(undefined);

	onMount(async () => {
		try {
			const data = await api.getLibraries();
			libraries = data.map((l) => ({
				id: l.id,
				name: l.name,
				book_count: l.book_count ?? 0,
				root_path: l.root_path,
				description: l.description,
			}));
		} catch {
			// Libraries will load when API is available
		}

		// Load recent reading books (top 3)
		try {
			const books = await api.getBooks({ reading_status: 'reading', sort_by: 'last_read_at', per_page: 3 });
			recentReading = (books?.data ?? []).map((b) => ({
				id: b.id,
				title: b.title,
				progress: b.progress ?? 0,
				cover_path: b.cover_path,
			}));
		} catch { /* optional */ }
	});

	const navGroups: NavGroup[] = [
		{
			label: '导航',
			items: [
				{ href: '/', label: '仪表盘', icon: LayoutDashboard },
				{ href: '/library', label: '所有书籍', icon: Library },
				{ href: '/series', label: '所有系列', icon: BookCopy },
				{ href: '/search', label: '搜索', icon: Search },
				{ href: '/discover', label: '探索', icon: Compass },
			],
		},
		{
			label: '知识',
			items: [
				{ href: '/ontology', label: '设定花园', icon: TreePine },
				{ href: '/graph', label: '知识图谱', icon: Network },
				{ href: '/characters', label: '人物志', icon: Users },
				{ href: '/semantic-tags', label: '智能标签', icon: Radar },
			],
		},
		{
			label: 'AI 工具',
			items: [
				{ href: '/chat', label: 'AI 对话', icon: MessageCircle },
				{ href: '/translate', label: '翻译', icon: Languages },
				{ href: '/analysis', label: '文本分析', icon: FileText },
				{ href: '/writing', label: '创作工坊', icon: Sparkles },
			],
		},
		{
			label: '管理',
			items: [
				{ href: '/stats', label: '统计', icon: BarChart3 },
				{ href: '/collections', label: '书单', icon: FolderOpen },
				{ href: '/tags', label: '标签', icon: Layers },
				{ href: '/persons', label: '作者', icon: PenTool },
				{ href: '/tasks', label: '任务队列', icon: ListChecks, adminOnly: true },
				{ href: '/admin', label: '管理面板', icon: Shield, adminOnly: true },
			],
		},
	];

	let isAdmin = $derived(auth.user?.role === 'admin');

	let filteredNavGroups = $derived(
		navGroups.map(g => ({
			...g,
			items: g.items.filter(item => !item.adminOnly || isAdmin),
		})).filter(g => g.items.length > 0)
	);

	function isActive(href: string, pathname: string): boolean {
		if (href === '/') return pathname === '/';
		return pathname === href || pathname.startsWith(`${href}/`);
	}

	function normalizeCoverPath(path: string | null | undefined): string | null {
		if (!path) return null;
		if (path.startsWith('/api/') || path.startsWith('http') || path.startsWith('data:')) return path;
		return `/api/covers/${path}`;
	}

	function isLibraryActive(libId: string): boolean {
		return $page.url.pathname.startsWith(`/libraries/${libId}`);
	}

	function openCreateDialog() {
		dialogMode = 'create';
		editingLibrary = undefined;
		dialogOpen = true;
	}

	function handleSaved(result: { id: string; name: string; root_path: string; description?: string | null }) {
		if (dialogMode === 'create' && result) {
			libraries = [...libraries, { id: result.id, name: result.name, book_count: 0, root_path: result.root_path, description: result.description }];
		} else if (dialogMode === 'edit' && result) {
			libraries = libraries.map(l => l.id === result.id ? { ...l, name: result.name, root_path: result.root_path, description: result.description } : l);
		}
	}

	async function scanLibrary(libId: string) {
		try {
			const result = await api.scanLibrary(libId);
			const parts: string[] = [];
			if (result.new_books) parts.push(`发现 ${result.new_books} 本新书`);
			if (result.series_detected) parts.push(`识别 ${result.series_detected} 个系列`);
			if (result.skipped_duplicates) parts.push(`跳过 ${result.skipped_duplicates} 重复`);
			if (result.errors) parts.push(`${result.errors} 个错误`);
			toast.success('扫描完成', { description: parts.join('，') || '无新增内容' });
			// Refresh library list
			const data = await api.getLibraries();
			libraries = data.map((l) => ({ id: l.id, name: l.name, book_count: l.book_count ?? 0, root_path: l.root_path, description: l.description }));
		} catch {
			toast.error('扫描失败');
		}
	}

	async function deleteLibrary(libId: string) {
		if (!confirm('确定删除此书库？')) return;
		try {
			await api.deleteLibrary(libId);
			libraries = libraries.filter(l => l.id !== libId);
		} catch { /* ignore */ }
	}
</script>

<LibraryDialog bind:open={dialogOpen} mode={dialogMode} library={editingLibrary} onclose={() => dialogOpen = false} onsaved={handleSaved} />

<aside
	class="flex h-screen flex-col overflow-hidden border-r border-ink-800/50 bg-ink-950 transition-all duration-300 ease-[cubic-bezier(0.4,0,0.2,1)]"
	class:w-64={!collapsed}
	class:w-16={collapsed}
>
	<!-- Logo — links to dashboard -->
	<a href="/" class="flex h-16 items-center gap-3 border-b border-ink-800/50 px-4 hover:bg-ink-900/50 transition-colors">
		<div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-accent-500/20 to-accent-600/10 ring-1 ring-accent-500/20">
			<BookMarked class="h-[18px] w-[18px] text-accent-400" />
		</div>
		{#if !collapsed}
			<span class="text-lg font-bold text-ink-50 tracking-tight">Nova Reader</span>
		{/if}
	</a>

	<!-- Navigation -->
	<nav class="min-h-0 flex-1 overflow-y-auto overscroll-contain px-2.5 py-3 [scrollbar-gutter:stable]">
		<!-- Libraries Section -->
		{#if !collapsed}
			<div class="flex items-center justify-between px-3 pb-1 pt-2">
				<a href="/libraries" class="text-[10px] font-semibold uppercase tracking-wider text-ink-500 transition-colors hover:text-ink-300">我的书库</a>
				{#if isAdmin}
					<button
						onclick={openCreateDialog}
						class="rounded p-0.5 text-ink-500 hover:text-accent-400 hover:bg-ink-800/50 transition-colors"
						title="新建书库"
						aria-label="新建书库"
					>
					<Plus size={12} strokeWidth={2.5} />
				</button>
				{/if}
			</div>
		{:else}
			<div class="flex justify-center py-2">
				<a href="/libraries" title="我的书库" aria-label="我的书库">
					<Library size={16} class="text-ink-500 transition-colors hover:text-ink-300" />
				</a>
			</div>
		{/if}

		<div class="space-y-0.5 mb-2">
			{#if libraries.length === 0}
				{#if !collapsed && isAdmin}
					<button
						onclick={openCreateDialog}
						class="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-xs text-ink-500 hover:text-accent-400 hover:bg-ink-800/50 transition-colors border border-dashed border-ink-800/50"
					>
						<Plus size={14} strokeWidth={2} />
						<span>新建书库</span>
					</button>
				{/if}
			{:else}
				{#each libraries as lib}
					{@const active = isLibraryActive(lib.id)}
					<div
						class="group relative flex items-center rounded-lg text-sm font-medium transition-all duration-150 {active ? 'bg-accent-500/10 text-accent-400' : 'text-ink-400 hover:bg-ink-800/60 hover:text-ink-200'}"
						title={collapsed ? `${lib.name} (${lib.book_count})` : undefined}
					>
						{#if active}
							<span class="absolute left-0 top-1/2 -translate-y-1/2 h-5 w-[3px] rounded-r-full bg-accent-500 transition-all"></span>
						{/if}
						<a
							href="/libraries/{lib.id}"
							class="flex min-w-0 flex-1 items-center gap-3 px-3 py-2"
							aria-current={active ? 'page' : undefined}
						>
							<Library size={16} strokeWidth={active ? 2.2 : 1.5} class="shrink-0" />
							{#if !collapsed}
								<span class="truncate flex-1">{lib.name}</span>
								<span class="text-[10px] text-ink-600 tabular-nums">{lib.book_count}</span>
							{/if}
						</a>
						{#if !collapsed && isAdmin}
							<DropdownMenu.Root>
								<DropdownMenu.Trigger
									class="mr-2 rounded p-0.5 text-ink-500 opacity-70 transition hover:bg-ink-700/50 hover:text-ink-200 hover:opacity-100 focus-visible:opacity-100"
									aria-label="打开 {lib.name} 的书库操作菜单"
								>
									<MoreHorizontal size={14} />
								</DropdownMenu.Trigger>
								<DropdownMenu.Content class="w-40 bg-ink-900 border-ink-800/60" align="start" side="right">
									<DropdownMenu.Item class="text-ink-300 hover:bg-ink-800/50 hover:text-ink-100 cursor-pointer" onclick={() => scanLibrary(lib.id)}>
										<RefreshCw size={13} class="mr-2" />
										扫描库文件
									</DropdownMenu.Item>
									<DropdownMenu.Item class="text-ink-300 hover:bg-ink-800/50 hover:text-ink-100 cursor-pointer" onclick={() => goto(`/libraries/${lib.id}/edit`)}>
										<Pencil size={13} class="mr-2" />
										编辑书库
									</DropdownMenu.Item>
									<DropdownMenu.Item class="text-ink-300 hover:bg-ink-800/50 hover:text-ink-100 cursor-pointer" onclick={() => goto(`/libraries/${lib.id}/analyze`)}>
										<Cpu size={13} class="mr-2" />
										AI 分析
									</DropdownMenu.Item>
									<DropdownMenu.Separator class="bg-ink-800/40" />
									<DropdownMenu.Item class="text-red-400 hover:bg-red-500/10 cursor-pointer" onclick={() => deleteLibrary(lib.id)}>
										<Trash2 size={13} class="mr-2" />
										删除书库
									</DropdownMenu.Item>
								</DropdownMenu.Content>
							</DropdownMenu.Root>
						{/if}
					</div>
				{/each}
			{/if}
		</div>

		<!-- Recent Reading -->
		{#if recentReading.length > 0}
			{#if !collapsed}
				<div class="flex items-center px-3 pb-1 pt-3">
					<span class="text-[10px] font-semibold uppercase tracking-wider text-ink-500">最近阅读</span>
				</div>
			{:else}
				<div class="flex justify-center py-2 mt-2">
					<BookOpen size={16} class="text-ink-500" />
				</div>
			{/if}
			<div class="space-y-0.5 mb-2">
				{#each recentReading as book}
					<a
						href="/reading/{book.id}"
						class="flex items-center gap-2.5 rounded-lg px-3 py-1.5 text-sm transition-colors hover:bg-ink-800/60 text-ink-400 hover:text-ink-200"
						title={collapsed ? `${book.title} (${Math.round(book.progress * 100)}%)` : undefined}
					>
						<div class="w-5 h-7 shrink-0 rounded-sm overflow-hidden bg-ink-800">
							{#if book.cover_path}
									<img src={normalizeCoverPath(book.cover_path)} alt="" class="w-full h-full object-cover" />
							{:else}
								<div class="w-full h-full flex items-center justify-center text-[8px] text-ink-600">📖</div>
							{/if}
						</div>
						{#if !collapsed}
							<div class="flex-1 min-w-0">
								<p class="text-xs text-ink-300 truncate">{book.title}</p>
								<div class="mt-0.5 h-1 w-full rounded-full bg-ink-800">
									<div class="h-full bg-accent-500/70 rounded-full" style="width: {Math.round(book.progress * 100)}%"></div>
								</div>
							</div>
						{/if}
					</a>
				{/each}
			</div>
		{/if}

		<!-- Static nav groups -->
		{#each filteredNavGroups as group, gi}
			<div class="my-2 border-t border-ink-800/30"></div>
			{#if !collapsed}
				<div class="px-3 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-wider text-ink-500">
					{group.label}
				</div>
			{/if}
			<div class="space-y-0.5">
				{#each group.items as item}
					{@const active = isActive(item.href, $page.url.pathname)}
					<a
						href={item.href}
						class="group relative flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-all duration-150 {active ? 'bg-accent-500/10 text-accent-400' : 'text-ink-400 hover:bg-ink-800/60 hover:text-ink-200'}"
						title={collapsed ? item.label : undefined}
						aria-current={active ? 'page' : undefined}
					>
						{#if active}
							<span class="absolute left-0 top-1/2 -translate-y-1/2 h-5 w-[3px] rounded-r-full bg-accent-500 transition-all"></span>
						{/if}
						<item.icon size={18} strokeWidth={active ? 2.2 : 1.5} class="shrink-0" />
						{#if !collapsed}
							<span class="truncate">{item.label}</span>
						{/if}
					</a>
				{/each}
			</div>
		{/each}
	</nav>

	<!-- Collapse Toggle -->
	<div class="border-t border-ink-800/50 p-2.5">
		<button
			onclick={() => collapsed = !collapsed}
			class="flex w-full items-center justify-center rounded-lg p-2 text-ink-500 hover:bg-ink-800/50 hover:text-ink-200 transition-colors"
			aria-label={collapsed ? '展开侧边栏' : '收起侧边栏'}
		>
			<ChevronsLeft
				size={18}
				class="transition-transform duration-300 {collapsed ? 'rotate-180' : ''}"
			/>
		</button>
	</div>
</aside>
