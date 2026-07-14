<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import LibraryDialog from '$components/layout/LibraryDialog.svelte';
	import { api } from '$services/api';
	import { auth } from '$stores/auth.svelte';
	import type { Library } from '$types/models';
	import {
		AlertCircle,
		BookOpen,
		Cpu,
		FolderPlus,
		HardDrive,
		Library as LibraryIcon,
		RefreshCw,
		Search,
		Settings2,
		Trash2,
	} from 'lucide-svelte';
	import { onMount } from 'svelte';
	import { toast } from 'svelte-sonner';

	let libraries = $state<Library[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let searchQuery = $state($page.url.searchParams.get('q') || '');
	let dialogOpen = $state(false);
	let dialogMode = $state<'create' | 'edit'>('create');
	let editingLibrary = $state<Library | undefined>(undefined);
	let scanningIds = $state<Set<string>>(new Set());
	let isAdmin = $derived(auth.user?.role === 'admin');

	let filteredLibraries = $derived(() => {
		const query = searchQuery.trim().toLowerCase();
		if (!query) return libraries;
		return libraries.filter((library) =>
			`${library.name} ${library.root_path} ${library.description ?? ''}`.toLowerCase().includes(query)
		);
	});

	let totals = $derived(() => ({
		books: libraries.reduce((sum, library) => sum + (library.book_count ?? 0), 0),
		series: libraries.reduce((sum, library) => sum + (library.series_count ?? 0), 0),
		bytes: libraries.reduce((sum, library) => sum + (library.total_size_bytes ?? 0), 0),
	}));

	$effect(() => {
		const params = new URLSearchParams($page.url.searchParams);
		if (searchQuery.trim()) params.set('q', searchQuery.trim());
		else params.delete('q');
		const qs = params.toString();
		const nextUrl = qs ? `/libraries?${qs}` : '/libraries';
		if (nextUrl !== $page.url.pathname + $page.url.search) {
			goto(nextUrl, { replaceState: true, keepFocus: true, noScroll: true });
		}
	});

	onMount(loadLibraries);

	function formatBytes(bytes: number): string {
		if (!bytes) return '0 B';
		const units = ['B', 'KB', 'MB', 'GB', 'TB'];
		const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
		return `${(bytes / Math.pow(1024, index)).toFixed(1)} ${units[index]}`;
	}

	async function loadLibraries() {
		loading = true;
		error = null;
		try {
			libraries = await api.getLibraries();
		} catch (err) {
			error = err instanceof Error ? err.message : '书库加载失败';
			libraries = [];
		} finally {
			loading = false;
		}
	}

	function openCreateDialog() {
		dialogMode = 'create';
		editingLibrary = undefined;
		dialogOpen = true;
	}

	function handleSaved() {
		void loadLibraries();
	}

	async function scanLibrary(libraryId: string) {
		if (scanningIds.has(libraryId)) return;
		scanningIds = new Set(scanningIds).add(libraryId);
		try {
			const result = await api.scanLibrary(libraryId);
			const details = [
				result.new_books ? `发现 ${result.new_books} 本新书` : null,
				result.series_detected ? `识别 ${result.series_detected} 个系列` : null,
				result.skipped_duplicates ? `跳过 ${result.skipped_duplicates} 个重复` : null,
				result.errors ? `${result.errors} 个错误` : null,
			].filter(Boolean).join('，');
			toast.success('扫描完成', { description: details || '没有新增内容' });
			await loadLibraries();
		} catch (err) {
			toast.error(err instanceof Error ? err.message : '扫描失败');
		} finally {
			const next = new Set(scanningIds);
			next.delete(libraryId);
			scanningIds = next;
		}
	}

	async function deleteLibrary(libraryId: string) {
		const library = libraries.find((item) => item.id === libraryId);
		if (!confirm(`确定删除书库「${library?.name ?? '此书库'}」？书籍文件不会被自动删除。`)) return;
		try {
			await api.deleteLibrary(libraryId);
			libraries = libraries.filter((item) => item.id !== libraryId);
			toast.success('书库已删除');
		} catch (err) {
			toast.error(err instanceof Error ? err.message : '删除失败');
		}
	}
</script>

<svelte:head>
	<title>Nova Reader — 书库管理</title>
</svelte:head>

<LibraryDialog bind:open={dialogOpen} mode={dialogMode} library={editingLibrary} onclose={() => dialogOpen = false} onsaved={handleSaved} />

<div class="mx-auto max-w-[1600px] space-y-6 px-4 py-6 sm:px-6 lg:px-8">
	<div class="flex flex-wrap items-center justify-between gap-4">
		<div class="flex items-center gap-3">
			<div class="flex h-11 w-11 items-center justify-center rounded-lg bg-accent-500/10 text-accent-400 ring-1 ring-accent-500/20">
				<LibraryIcon size={22} strokeWidth={1.8} />
			</div>
			<div>
				<h1 class="text-2xl font-bold text-ink-50">书库管理</h1>
				<p class="mt-1 text-sm text-ink-400">管理目录、扫描规则、权限和书库级 AI 能力</p>
			</div>
		</div>

		<div class="flex items-center gap-2">
			<button
				type="button"
				onclick={loadLibraries}
				class="inline-flex items-center gap-2 rounded-lg border border-ink-700/50 px-3 py-2 text-sm text-ink-300 transition-colors hover:bg-ink-800/50 hover:text-ink-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 disabled:opacity-50"
				disabled={loading}
			>
				<RefreshCw size={15} class={loading ? 'animate-spin' : ''} />
				刷新
			</button>
			{#if isAdmin}
				<button
					type="button"
					onclick={openCreateDialog}
					class="inline-flex items-center gap-2 rounded-lg bg-accent-500 px-3 py-2 text-sm font-medium text-ink-950 transition-colors hover:bg-accent-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-300/80"
				>
					<FolderPlus size={15} />
					新建书库
				</button>
			{/if}
		</div>
	</div>

	<div class="grid gap-3 sm:grid-cols-3">
		<div class="rounded-lg border border-ink-800/50 bg-ink-900/50 p-4">
			<p class="text-xs text-ink-500">书库</p>
			<p class="mt-1 text-2xl font-semibold text-ink-100">{libraries.length}</p>
		</div>
		<div class="rounded-lg border border-ink-800/50 bg-ink-900/50 p-4">
			<p class="text-xs text-ink-500">书籍 / 系列</p>
			<p class="mt-1 text-2xl font-semibold text-ink-100">{totals().books} / {totals().series}</p>
		</div>
		<div class="rounded-lg border border-ink-800/50 bg-ink-900/50 p-4">
			<p class="text-xs text-ink-500">存储占用</p>
			<p class="mt-1 text-2xl font-semibold text-ink-100">{formatBytes(totals().bytes)}</p>
		</div>
	</div>

	<div class="flex flex-wrap items-center justify-between gap-3">
		<div class="relative w-full sm:max-w-sm">
			<Search size={15} class="absolute left-3 top-1/2 -translate-y-1/2 text-ink-500" />
			<input
				bind:value={searchQuery}
				type="search"
				name="library-management-search"
				autocomplete="off"
				aria-label="筛选书库"
				placeholder="筛选书库…"
				class="w-full rounded-lg border border-ink-700/50 bg-ink-900/50 py-2 pl-9 pr-3 text-sm text-ink-100 transition-colors placeholder:text-ink-600 focus:border-accent-500/40 focus:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/60"
			/>
		</div>
		<a href="/library" class="inline-flex items-center gap-2 rounded-lg px-3 py-2 text-sm text-ink-400 transition-colors hover:bg-ink-800/50 hover:text-ink-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70">
			<BookOpen size={15} />
			查看所有书籍
		</a>
	</div>

	{#if loading}
		<div class="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
			{#each Array(6) as _}
				<div class="h-52 animate-pulse rounded-lg border border-ink-800/50 bg-ink-900/40"></div>
			{/each}
		</div>
	{:else if error}
		<div class="rounded-lg border border-red-500/20 bg-red-500/5 p-10 text-center">
			<AlertCircle class="mx-auto mb-3 h-9 w-9 text-red-400" />
			<p class="text-sm text-red-300">{error}</p>
			<button type="button" onclick={loadLibraries} class="mt-4 rounded-lg bg-ink-800 px-4 py-2 text-sm text-ink-100 transition-colors hover:bg-ink-700 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70">重试</button>
		</div>
	{:else if filteredLibraries().length === 0}
		<div class="rounded-lg border border-dashed border-ink-800 bg-ink-900/30 p-12 text-center">
			<LibraryIcon class="mx-auto mb-4 h-12 w-12 text-ink-600" strokeWidth={1.4} />
			<h2 class="text-lg font-semibold text-ink-200">{libraries.length === 0 ? '还没有书库' : '没有匹配的书库'}</h2>
			<p class="mt-1 text-sm text-ink-500">{libraries.length === 0 ? '新建一个书库后，Nova 会从目录扫描并组织你的书籍。' : '换个关键词试试。'}</p>
			{#if libraries.length === 0 && isAdmin}
				<button type="button" onclick={openCreateDialog} class="mt-5 inline-flex items-center gap-2 rounded-lg bg-accent-500 px-4 py-2 text-sm font-medium text-ink-950 transition-colors hover:bg-accent-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-300/80">
					<FolderPlus size={15} />
					新建书库
				</button>
			{/if}
		</div>
	{:else}
		<div class="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
			{#each filteredLibraries() as lib}
				<article class="group relative rounded-lg border border-ink-800/50 bg-ink-900/70 p-5 transition-colors hover:border-accent-500/25">
					<a href="/libraries/{lib.id}" class="absolute inset-0 z-0 rounded-lg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70" aria-label="打开书库 {lib.name}"></a>
					<div class="relative z-10 flex items-start justify-between gap-3">
						<div class="min-w-0">
							<h2 class="truncate text-lg font-semibold text-ink-100 group-hover:text-accent-300">{lib.name}</h2>
							<p class="mt-1 truncate font-mono text-xs text-ink-500" title={lib.root_path}>{lib.root_path}</p>
						</div>
						<span class="shrink-0 rounded-full px-2 py-0.5 text-xs {lib.scan_status === 'scanning' ? 'bg-amber-500/10 text-amber-300' : 'bg-ink-800 text-ink-400'}">
							{lib.scan_status === 'scanning' ? '扫描中' : '空闲'}
						</span>
					</div>

					{#if lib.description}
						<p class="relative z-10 mt-3 line-clamp-2 text-sm text-ink-400">{lib.description}</p>
					{/if}

					<div class="relative z-10 mt-5 grid grid-cols-3 gap-3">
						<div>
							<p class="text-xl font-semibold text-ink-100">{lib.book_count ?? 0}</p>
							<p class="text-xs text-ink-500">书籍</p>
						</div>
						<div>
							<p class="text-xl font-semibold text-ink-100">{lib.series_count ?? 0}</p>
							<p class="text-xs text-ink-500">系列</p>
						</div>
						<div>
							<p class="truncate text-xl font-semibold text-ink-100">{formatBytes(lib.total_size_bytes ?? 0)}</p>
							<p class="text-xs text-ink-500">大小</p>
						</div>
					</div>

					<div class="relative z-10 mt-5 flex items-center justify-between gap-3 border-t border-ink-800/50 pt-4">
						<div class="flex min-w-0 items-center gap-2 text-xs text-ink-500">
							<HardDrive size={13} />
							<span class="truncate">{lib.last_scan_at ? `上次扫描 ${new Date(lib.last_scan_at).toLocaleString('zh-CN')}` : '尚未扫描'}</span>
						</div>
						{#if isAdmin}
							<div class="flex shrink-0 items-center gap-1">
								<button type="button" onclick={() => scanLibrary(lib.id)} class="rounded-md p-1.5 text-ink-500 transition-colors hover:bg-ink-800 hover:text-accent-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 disabled:opacity-50" title="扫描" aria-label="扫描 {lib.name}" aria-busy={scanningIds.has(lib.id)} disabled={scanningIds.has(lib.id)}>
									<RefreshCw size={15} class={scanningIds.has(lib.id) ? 'animate-spin' : ''} />
								</button>
								<button type="button" onclick={() => goto(`/libraries/${lib.id}/analyze`)} class="rounded-md p-1.5 text-ink-500 transition-colors hover:bg-ink-800 hover:text-accent-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70" title="AI 分析" aria-label="AI 分析 {lib.name}">
									<Cpu size={15} />
								</button>
								<button type="button" onclick={() => goto(`/libraries/${lib.id}/edit`)} class="rounded-md p-1.5 text-ink-500 transition-colors hover:bg-ink-800 hover:text-ink-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70" title="高级设置" aria-label="高级设置 {lib.name}">
									<Settings2 size={15} />
								</button>
								<button type="button" onclick={() => deleteLibrary(lib.id)} class="rounded-md p-1.5 text-ink-500 transition-colors hover:bg-red-500/10 hover:text-red-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red-300/70" title="删除" aria-label="删除 {lib.name}">
									<Trash2 size={15} />
								</button>
							</div>
						{/if}
					</div>
				</article>
			{/each}
		</div>
	{/if}
</div>
