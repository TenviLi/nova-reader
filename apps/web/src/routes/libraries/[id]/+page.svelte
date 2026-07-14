<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { api } from '$services/api';
	import { onMount } from 'svelte';
	import { BookOpen, Layers, Clock, RefreshCw, ChevronRight, Library as LibraryIcon, Settings2, BookMarked, Filter, ArrowUpDown, Grid3X3, List, Cpu, Tag, User, Upload } from 'lucide-svelte';
	import { toast } from 'svelte-sonner';
	import { Input } from '$lib/components/ui/input';
	import { Button } from '$lib/components/ui/button';
	import BookCard from '$lib/components/library/BookCard.svelte';

	import type { Book, Library, Series } from '$types/models';

	const libraryId = $page.params.id!;
	type LibraryTab = 'overview' | 'books' | 'series' | 'tags' | 'authors';
	type BooksSort = 'title' | 'author' | 'updated_at' | 'created_at';
	type BooksViewMode = 'grid' | 'list';
	const tabs = [
		{ key: 'overview', label: '总览', icon: BookMarked },
		{ key: 'books', label: '本库书籍', icon: BookOpen },
		{ key: 'series', label: '系列', icon: Layers },
		{ key: 'tags', label: '标签', icon: Tag },
		{ key: 'authors', label: '作者', icon: User },
	] as const;

	let library = $state<Library | null>(null);
	let continueReading = $state<Book[]>([]);
	let recentBooks = $state<Book[]>([]);
	let allBooks = $state<Book[]>([]);
	let series = $state<Series[]>([]);
	let libraryTags = $state<Array<{name: string; count: number}>>([]);
	let libraryAuthors = $state<Array<{name: string; count: number}>>([]);
	let loading = $state(true);
	let activeTab = $state<LibraryTab>(normalizeTab($page.url.searchParams.get('tab')));
	let uploadInput = $state<HTMLInputElement | null>(null);
	let uploading = $state(false);

	// Books tab state
	let booksFilter = $state($page.url.searchParams.get('q') || '');
	let booksSort = $state<BooksSort>(normalizeBooksSort($page.url.searchParams.get('sort')));
	let booksViewMode = $state<BooksViewMode>(normalizeBooksView($page.url.searchParams.get('view')));
	let activeLetter = $state<string | null>($page.url.searchParams.get('letter'));

	// Series tab state
	let seriesFilter = $state($page.url.searchParams.get('series_q') || '');
	let seriesLetter = $state<string | null>($page.url.searchParams.get('series_letter'));

	const alphabet = '#ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('');

	function normalizeTab(tab: string | null): LibraryTab {
		return tabs.some((item) => item.key === tab) ? tab as LibraryTab : 'overview';
	}

	function normalizeBooksSort(sort: string | null): BooksSort {
		return ['title', 'author', 'updated_at', 'created_at'].includes(sort ?? '')
			? sort as BooksSort
			: 'updated_at';
	}

	function normalizeBooksView(view: string | null): BooksViewMode {
		return view === 'list' ? 'list' : 'grid';
	}

	function setActiveTab(tab: LibraryTab) {
		activeTab = tab;
		const params = new URLSearchParams($page.url.searchParams);
		if (tab === 'overview') params.delete('tab');
		else params.set('tab', tab);
		const qs = params.toString();
		goto(qs ? `/libraries/${libraryId}?${qs}` : `/libraries/${libraryId}`, {
			replaceState: true,
			keepFocus: true,
			noScroll: true,
		});
	}

	function normalizeCoverPath(path: string | null | undefined): string | null {
		if (!path) return null;
		if (path.startsWith('/api/') || path.startsWith('http') || path.startsWith('data:')) return path;
		return `/api/covers/${path}`;
	}

	async function loadAllLibraryBooks() {
		const perPage = 200;
		let pageNumber = 1;
		let totalPages = 1;
		const books: Book[] = [];

		do {
			const response = await api.getBooks({
				library_id: libraryId,
				page: pageNumber,
				per_page: perPage,
				sort_by: 'updated_at',
			});
			books.push(...(response.data ?? response.items ?? []));
			totalPages = response.total_pages || Math.ceil((response.total ?? books.length) / perPage) || pageNumber;
			pageNumber += 1;
		} while (pageNumber <= totalPages);

		return books;
	}

	function setLibraryBooks(booksData: Book[]) {
		allBooks = booksData;
		continueReading = allBooks.filter(b => b.reading_status === 'reading').slice(0, 6);
		recentBooks = allBooks.slice(0, 12);

		const tagMap = new Map<string, number>();
		for (const book of allBooks) {
			for (const t of (book.tags ?? [])) {
				tagMap.set(t, (tagMap.get(t) ?? 0) + 1);
			}
		}
		libraryTags = [...tagMap.entries()]
			.map(([name, count]) => ({ name, count }))
			.sort((a, b) => b.count - a.count);

		const authorMap = new Map<string, number>();
		for (const book of allBooks) {
			const author = book.author ?? '未知作者';
			authorMap.set(author, (authorMap.get(author) ?? 0) + 1);
		}
		libraryAuthors = [...authorMap.entries()]
			.map(([name, count]) => ({ name, count }))
			.sort((a, b) => b.count - a.count);
	}

	$effect(() => {
		const tab = normalizeTab($page.url.searchParams.get('tab'));
		if (tab !== activeTab) activeTab = tab;
	});

	$effect(() => {
		const params = new URLSearchParams($page.url.searchParams);
		if (activeTab === 'overview') params.delete('tab');
		else params.set('tab', activeTab);
		if (booksFilter.trim()) params.set('q', booksFilter.trim());
		else params.delete('q');
		if (booksSort !== 'updated_at') params.set('sort', booksSort);
		else params.delete('sort');
		if (booksViewMode !== 'grid') params.set('view', booksViewMode);
		else params.delete('view');
		if (activeLetter) params.set('letter', activeLetter);
		else params.delete('letter');
		if (seriesFilter.trim()) params.set('series_q', seriesFilter.trim());
		else params.delete('series_q');
		if (seriesLetter) params.set('series_letter', seriesLetter);
		else params.delete('series_letter');
		const qs = params.toString();
		const nextUrl = qs ? `/libraries/${libraryId}?${qs}` : `/libraries/${libraryId}`;
		if (nextUrl !== $page.url.pathname + $page.url.search) {
			goto(nextUrl, { replaceState: true, keepFocus: true, noScroll: true });
		}
	});

	let filteredBooks = $derived(() => {
		let list = allBooks;
		// Text filter
		if (booksFilter.trim()) {
			const q = booksFilter.toLowerCase();
			list = list.filter(b => b.title?.toLowerCase().includes(q) || b.author?.toLowerCase().includes(q));
		}
		// Letter filter
		if (activeLetter) {
			if (activeLetter === '#') {
				list = list.filter(b => !/^[a-zA-Z\u4e00-\u9fff]/.test(b.title ?? ''));
			} else {
				list = list.filter(b => {
					const first = (b.title ?? '')[0]?.toUpperCase();
					return first === activeLetter;
				});
			}
		}
		// Sort
		list = [...list].sort((a, b) => {
			if (booksSort === 'title') return (a.title ?? '').localeCompare(b.title ?? '', 'zh-CN');
			if (booksSort === 'author') return (a.author ?? '').localeCompare(b.author ?? '', 'zh-CN');
			if (booksSort === 'updated_at') return new Date(b.updated_at ?? 0).getTime() - new Date(a.updated_at ?? 0).getTime();
			return new Date(b.created_at ?? 0).getTime() - new Date(a.created_at ?? 0).getTime();
		});
		return list;
	});

	let filteredSeries = $derived(() => {
		let list = series;
		if (seriesFilter.trim()) {
			const q = seriesFilter.toLowerCase();
			list = list.filter(s => s.name?.toLowerCase().includes(q) || s.author?.toLowerCase().includes(q));
		}
		if (seriesLetter) {
			if (seriesLetter === '#') {
				list = list.filter(s => !/^[a-zA-Z\u4e00-\u9fff]/.test(s.name ?? ''));
			} else {
				list = list.filter(s => (s.name ?? '')[0]?.toUpperCase() === seriesLetter);
			}
		}
		return [...list].sort((a, b) => (a.name ?? '').localeCompare(b.name ?? '', 'zh-CN'));
	});

	onMount(async () => {
		try {
			const [libData, booksData] = await Promise.all([
				api.getLibrary(libraryId),
				loadAllLibraryBooks(),
			]);
			library = libData ?? null;

			setLibraryBooks(booksData);

			// Get series for this library
			try {
				const seriesData = await api.getSeriesByLibrary(libraryId);
				series = seriesData ?? [];
			} catch { /* series optional */ }
		} catch (e: unknown) {
			toast.error('加载书库失败');
		} finally {
			loading = false;
		}
	});

	async function handleScan() {
		try {
			const result = await api.scanLibrary(libraryId);
			const parts: string[] = [];
			if (result.new_books) parts.push(`发现 ${result.new_books} 本新书`);
			if (result.series_detected) parts.push(`识别 ${result.series_detected} 个系列`);
			if (result.errors) parts.push(`${result.errors} 个错误`);
			toast.success('扫描完成', { description: parts.join('，') || '无新增内容' });
		} catch {
			toast.error('扫描失败');
		}
	}

	async function handleUploadBooks(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const files = Array.from(input.files ?? []);
		if (files.length === 0 || uploading) return;

		uploading = true;
		try {
			for (const file of files) {
				await api.uploadBook(file, libraryId);
			}
			setLibraryBooks(await loadAllLibraryBooks());
			setActiveTab('books');
			toast.success('上传任务已提交', {
				description: files.length === 1 ? files[0].name : `${files.length} 个文件已加入处理队列`,
			});
		} catch (e: unknown) {
			const message = e instanceof Error ? e.message : '请稍后重试';
			toast.error('上传失败', { description: message });
		} finally {
			uploading = false;
			input.value = '';
		}
	}
</script>

<svelte:head>
	<title>Nova Reader — {library?.name ?? '书库'}</title>
</svelte:head>

<div class="mx-auto max-w-[1600px] px-4 py-6 sm:px-6 lg:px-8 space-y-6 animate-fade-in">
	{#if loading}
		<div class="space-y-4">
			<div class="h-10 w-48 rounded-lg bg-ink-900/50 animate-pulse"></div>
			<div class="grid grid-cols-3 gap-4">
				{#each Array(3) as _}
					<div class="h-24 rounded-xl bg-ink-900/50 animate-pulse"></div>
				{/each}
			</div>
		</div>
		{:else if library}
			<!-- Library Header -->
			<div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
				<div class="flex min-w-0 items-center gap-3">
					<div class="flex h-10 w-10 items-center justify-center rounded-xl bg-accent-500/10 ring-1 ring-accent-500/20">
						<LibraryIcon size={20} class="text-accent-400" />
					</div>
					<div class="min-w-0">
						<a href="/libraries" aria-label="返回书库管理" class="mb-1 inline-flex text-xs text-ink-500 transition-colors hover:text-accent-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70">
							书库管理
						</a>
						<h1 class="truncate text-2xl font-bold text-ink-50">{library.name}</h1>
						<p class="truncate text-sm text-ink-400" title={library.root_path}>{library.root_path}</p>
					</div>
				</div>
				<div class="flex flex-wrap items-center gap-2 sm:justify-end">
					<input
						bind:this={uploadInput}
						type="file"
						accept=".epub,.pdf,.txt,.md,.mobi,.azw3,.docx"
						multiple
						class="hidden"
						aria-hidden="true"
						tabindex="-1"
						onchange={handleUploadBooks}
					/>
					<Button variant="outline" size="sm" onclick={() => uploadInput?.click()} disabled={uploading} aria-label="向当前书库上传书籍">
						<Upload size={14} />
						{uploading ? '上传中...' : '上传书籍'}
					</Button>
					<Button variant="outline" size="sm" onclick={handleScan}>
						<RefreshCw size={14} />
						扫描
					</Button>
					<Button variant="outline" size="sm" href="/libraries/{libraryId}/analyze">
						<Cpu size={14} />
						AI 分析
					</Button>
					<Button variant="outline" size="sm" href="/libraries/{libraryId}/edit">
						<Settings2 size={14} />
						设置
					</Button>
				</div>
			</div>

		<!-- Tab Navigation -->
		<div class="flex gap-1 overflow-x-auto border-b border-ink-800/50" role="tablist" aria-label="书库工作区视图">
			{#each tabs as tab}
				<button
					type="button"
					role="tab"
					aria-selected={activeTab === tab.key}
					onclick={() => setActiveTab(tab.key)}
					class="flex shrink-0 items-center gap-1.5 px-4 py-2.5 text-sm font-medium border-b-2 -mb-[1px] transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 {activeTab === tab.key ? 'border-accent-500 text-accent-400' : 'border-transparent text-ink-400 hover:text-ink-200'}"
				>
					<tab.icon size={15} />
					{tab.label}
				</button>
			{/each}
		</div>

		<!-- Tab Content -->
		{#if activeTab === 'overview'}
			<!-- Stats -->
			<div class="grid grid-cols-2 md:grid-cols-4 gap-4">
				<div class="rounded-xl border border-ink-800/50 bg-ink-900/50 p-4 text-center">
					<p class="text-2xl font-bold text-ink-50">{library.book_count ?? 0}</p>
					<p class="text-xs text-ink-400 mt-1">书籍</p>
				</div>
				<div class="rounded-xl border border-ink-800/50 bg-ink-900/50 p-4 text-center">
					<p class="text-2xl font-bold text-ink-50">{series.length}</p>
					<p class="text-xs text-ink-400 mt-1">系列</p>
				</div>
				<div class="rounded-xl border border-ink-800/50 bg-ink-900/50 p-4 text-center">
					<p class="text-2xl font-bold text-ink-50">{continueReading.length}</p>
					<p class="text-xs text-ink-400 mt-1">阅读中</p>
				</div>
				<div class="rounded-xl border border-ink-800/50 bg-ink-900/50 p-4 text-center">
					<p class="text-2xl font-bold text-ink-50">
						{library.last_scan_at ? new Date(library.last_scan_at).toLocaleDateString('zh-CN') : '—'}
					</p>
					<p class="text-xs text-ink-400 mt-1">上次扫描</p>
				</div>
			</div>

			<!-- Continue Reading -->
			{#if continueReading.length > 0}
				<section>
					<div class="flex items-center justify-between mb-3">
						<h2 class="text-lg font-semibold text-ink-100">继续阅读</h2>
					</div>
					<div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 gap-4">
						{#each continueReading as book}
							<BookCard {book} />
						{/each}
					</div>
				</section>
			{/if}

			<!-- Recently Added -->
			<section>
				<div class="flex items-center justify-between mb-3">
					<h2 class="text-lg font-semibold text-ink-100">最新入库</h2>
					<button type="button" onclick={() => setActiveTab('books')} aria-label="查看本库全部书籍" class="text-xs text-accent-400 hover:text-accent-300 flex items-center gap-0.5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70">
						查看全部 <ChevronRight size={12} />
					</button>
				</div>
				<div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 gap-4">
					{#each recentBooks as book}
						<BookCard {book} />
					{/each}
				</div>
			</section>

			<!-- Series -->
			{#if series.length > 0}
				<section>
					<div class="flex items-center justify-between mb-3">
						<h2 class="text-lg font-semibold text-ink-100">系列</h2>
						<button type="button" onclick={() => setActiveTab('series')} aria-label="查看本库全部系列" class="text-xs text-accent-400 hover:text-accent-300 flex items-center gap-0.5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70">
							查看全部 <ChevronRight size={12} />
						</button>
					</div>
					<div class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4">
						{#each series as s}
							<a href="/series/{s.id}" class="rounded-xl border border-ink-800/50 bg-ink-900/50 p-4 transition-colors hover:border-accent-500/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 group">
								<div class="flex items-center gap-3">
									<Layers size={18} class="text-ink-500 group-hover:text-accent-400" />
									<div class="min-w-0">
										<p class="text-sm font-medium text-ink-100 truncate">{s.name}</p>
										<p class="text-xs text-ink-500">{s.book_count ?? 0} 本</p>
									</div>
								</div>
							</a>
						{/each}
					</div>
				</section>
			{/if}

		{:else if activeTab === 'books'}
			<!-- A-Z Filter Bar -->
			<div class="flex flex-wrap items-center gap-1 mb-4">
				{#each alphabet as letter}
					<button
						type="button"
						onclick={() => activeLetter = activeLetter === letter ? null : letter}
						aria-label={`筛选本库书籍首字母 ${letter}`}
						aria-pressed={activeLetter === letter}
						class="w-7 h-7 shrink-0 rounded text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 {activeLetter === letter ? 'bg-accent-500 text-ink-950' : 'text-ink-400 hover:bg-ink-800/50 hover:text-ink-200'}"
					>
						{letter}
					</button>
				{/each}
				{#if activeLetter}
					<button type="button" onclick={() => activeLetter = null} aria-label="清除本库书籍首字母筛选" class="ml-2 text-xs text-accent-400 transition-colors hover:text-accent-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70">清除</button>
				{/if}
			</div>

			<!-- Toolbar -->
			<div class="flex flex-col items-stretch gap-3 mb-4 sm:flex-row sm:items-center">
				<Input
					bind:value={booksFilter}
					aria-label="搜索书名或作者"
					name="library-book-search"
					autocomplete="off"
					placeholder="搜索书名或作者…"
					class="w-full bg-ink-800/50 border-ink-700/60 text-ink-100 placeholder:text-ink-600 sm:max-w-xs"
				/>
				<select
					bind:value={booksSort}
					name="library-book-sort"
					aria-label="本库书籍排序"
					class="h-8 rounded-lg border border-ink-700/60 bg-ink-800/50 px-3 text-sm text-ink-200 transition-colors focus:border-accent-500/50 focus:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/60"
				>
					<option value="updated_at">最近更新</option>
					<option value="created_at">入库时间</option>
					<option value="title">按标题</option>
					<option value="author">按作者</option>
				</select>
				<div class="flex items-center gap-1 sm:ml-auto">
					<button
						type="button"
						onclick={() => booksViewMode = 'grid'}
						aria-label="网格视图"
						aria-pressed={booksViewMode === 'grid'}
						class="rounded p-1.5 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 {booksViewMode === 'grid' ? 'bg-ink-800 text-ink-100' : 'text-ink-500 hover:text-ink-300'}"
					>
						<Grid3X3 size={16} />
					</button>
					<button
						type="button"
						onclick={() => booksViewMode = 'list'}
						aria-label="列表视图"
						aria-pressed={booksViewMode === 'list'}
						class="rounded p-1.5 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 {booksViewMode === 'list' ? 'bg-ink-800 text-ink-100' : 'text-ink-500 hover:text-ink-300'}"
					>
						<List size={16} />
					</button>
				</div>
			</div>

			<!-- Results count -->
			<p class="text-xs text-ink-500 mb-3">{filteredBooks().length} 本书籍</p>

			<!-- Book Grid/List -->
			{#if booksViewMode === 'grid'}
				<div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 gap-4">
					{#each filteredBooks() as book}
						<BookCard {book} />
					{/each}
				</div>
			{:else}
				<div class="space-y-1">
					{#each filteredBooks() as book}
						<a href="/library/{book.id}" class="flex items-center gap-4 rounded-lg px-3 py-2.5 hover:bg-ink-800/50 transition-colors group">
							<div class="h-12 w-9 shrink-0 rounded bg-ink-800 overflow-hidden ring-1 ring-ink-700/50">
								{#if book.cover_path}
									<img src={normalizeCoverPath(book.cover_path)} alt="" width="36" height="48" class="h-full w-full object-cover" loading="lazy" />
								{:else}
									<div class="flex h-full items-center justify-center">
										<BookOpen size={12} class="text-ink-600" />
									</div>
								{/if}
							</div>
							<div class="min-w-0 flex-1">
								<p class="text-sm text-ink-200 truncate group-hover:text-accent-400 transition-colors">{book.title}</p>
								<p class="text-xs text-ink-500 truncate">{book.author ?? ''}</p>
							</div>
							<span class="text-xs text-ink-600 shrink-0">{book.chapter_count ?? 0} 章</span>
						</a>
					{/each}
				</div>
			{/if}

			{#if filteredBooks().length === 0}
				<div class="text-center py-12">
					<BookOpen size={36} class="mx-auto text-ink-600 mb-2" />
					<p class="text-ink-400">未找到匹配的书籍</p>
				</div>
			{/if}

		{:else if activeTab === 'series'}
			<!-- A-Z Filter Bar for Series -->
			<div class="flex flex-wrap items-center gap-1 mb-4">
				{#each alphabet as letter}
					<button
						type="button"
						onclick={() => seriesLetter = seriesLetter === letter ? null : letter}
						aria-label={`筛选本库系列首字母 ${letter}`}
						aria-pressed={seriesLetter === letter}
						class="w-7 h-7 shrink-0 rounded text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 {seriesLetter === letter ? 'bg-accent-500 text-ink-950' : 'text-ink-400 hover:bg-ink-800/50 hover:text-ink-200'}"
					>
						{letter}
					</button>
				{/each}
				{#if seriesLetter}
					<button type="button" onclick={() => seriesLetter = null} aria-label="清除本库系列首字母筛选" class="ml-2 text-xs text-accent-400 transition-colors hover:text-accent-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70">清除</button>
				{/if}
			</div>

			<div class="mb-4">
				<Input
					bind:value={seriesFilter}
					aria-label="搜索系列名"
					name="library-series-search"
					autocomplete="off"
					placeholder="搜索系列名…"
					class="max-w-xs bg-ink-800/50 border-ink-700/60 text-ink-100 placeholder:text-ink-600"
				/>
			</div>

			<p class="text-xs text-ink-500 mb-3">{filteredSeries().length} 个系列</p>

			<div class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-4">
				{#each filteredSeries() as s}
					{@const seriesBooks = allBooks.filter(b => b.series === s.name).slice(0, 4)}
					<a href="/series/{s.id}" class="rounded-xl border border-ink-800/50 bg-ink-900/50 p-5 transition-colors hover:border-accent-500/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 group">
						<!-- Cover mosaic -->
						<div class="mb-3 grid grid-cols-4 gap-1 h-20 overflow-hidden rounded-lg">
							{#each seriesBooks as book}
								<div class="bg-ink-800 rounded overflow-hidden">
									{#if book.cover_path}
										<img src={normalizeCoverPath(book.cover_path)} alt="" width="48" height="80" class="h-full w-full object-cover" loading="lazy" />
									{:else}
										<div class="h-full w-full flex items-center justify-center text-[8px] text-ink-600">📖</div>
									{/if}
								</div>
							{/each}
							{#each Array(Math.max(0, 4 - seriesBooks.length)) as _}
								<div class="bg-ink-800/40 rounded"></div>
							{/each}
						</div>
						<h3 class="font-medium text-ink-100 truncate group-hover:text-accent-400 transition-colors">{s.name}</h3>
						<p class="text-sm text-ink-400 mt-1">{s.book_count ?? 0} 本 · {s.author ?? '未知作者'}</p>
					</a>
				{/each}
			</div>
			{#if filteredSeries().length === 0}
				<div class="text-center py-12">
					<Layers size={36} class="mx-auto text-ink-600 mb-2" />
					<p class="text-ink-400">未找到匹配的系列</p>
				</div>
			{/if}

		{:else if activeTab === 'tags'}
			{#if libraryTags.length === 0}
				<div class="text-center py-12">
					<Tag size={36} class="mx-auto text-ink-600 mb-2" />
					<p class="text-ink-400">暂无标签</p>
					<p class="text-xs text-ink-500 mt-1">通过 AI 分析为书籍生成标签</p>
				</div>
			{:else}
				<div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-3">
					{#each libraryTags as tag}
						<div class="flex items-center gap-3 rounded-lg border border-ink-700/40 bg-ink-900/30 px-4 py-3">
							<Tag size={14} class="text-ink-500 shrink-0" />
							<div class="flex-1 min-w-0">
								<p class="text-sm text-ink-200 truncate">{tag.name}</p>
								<p class="text-xs text-ink-500">{tag.count} 本</p>
							</div>
						</div>
					{/each}
				</div>
			{/if}

		{:else if activeTab === 'authors'}
			{#if libraryAuthors.length === 0}
				<div class="text-center py-12">
					<User size={36} class="mx-auto text-ink-600 mb-2" />
					<p class="text-ink-400">暂无作者信息</p>
				</div>
			{:else}
				<div class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-4">
					{#each libraryAuthors as author}
						<a href="/persons?q={encodeURIComponent(author.name)}" class="flex items-center gap-3 rounded-xl border border-ink-800/50 bg-ink-900/50 p-4 transition-colors hover:border-accent-500/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 group">
							<div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-ink-800/80">
								<User size={16} class="text-ink-500 group-hover:text-accent-400 transition-colors" />
							</div>
							<div class="min-w-0 flex-1">
								<p class="text-sm font-medium text-ink-200 truncate group-hover:text-accent-400 transition-colors">{author.name}</p>
								<p class="text-xs text-ink-500">{author.count} 本书</p>
							</div>
						</a>
					{/each}
				</div>
			{/if}

		{/if}
	{:else}
		<div class="text-center py-20">
			<LibraryIcon size={48} class="mx-auto text-ink-600 mb-3" />
			<p class="text-ink-300">书库不存在或已被删除</p>
			<a href="/libraries" class="mt-3 inline-block text-sm text-accent-400 hover:text-accent-300">返回书库管理</a>
		</div>
	{/if}
</div>
