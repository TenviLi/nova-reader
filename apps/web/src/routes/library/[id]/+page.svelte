<script lang="ts">
	import { getErrorMessage } from '$lib/utils';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { api } from '$services/api';
	import { toast } from 'svelte-sonner';
	import type { Book, Chapter, ChapterTitleResult, CommunitySummaryResult, Entity, Annotation, Language, PlotHoleReport, ReadingStatus } from '$types/models';
	import { AlertTriangle, BarChart3, BookOpen, Brain, Check, RefreshCw, Lightbulb, PenSquare, Download, Trash2, FolderPlus, Network, Plus, Search, Sparkles, X } from 'lucide-svelte';
	import SimilarBooks from '$components/library/SimilarBooks.svelte';
	import AddToCollectionDialog from '$components/library/AddToCollectionDialog.svelte';
	import SemanticHeatmap from '$components/analytics/SemanticHeatmap.svelte';
	import SemanticMarkersPanel from '$components/analytics/SemanticMarkersPanel.svelte';
	import SemanticRadar from '$components/analytics/SemanticRadar.svelte';
	import BookAnalysisPanel from '$components/analytics/BookAnalysisPanel.svelte';
	import { onMount } from 'svelte';

	type AnnotationExportFormat = 'markdown' | 'json' | 'notion';

	let bookId = $derived($page.params.id!);
	let book = $state<Book | null>(null);
	let chapters = $state.raw<Chapter[]>([]);
	let entities = $state.raw<Entity[]>([]);
	let annotations = $state.raw<Annotation[]>([]);
	let loading = $state(true);
	let activeTab = $state<'info' | 'chapters' | 'entities' | 'annotations' | 'similar' | 'characters' | 'semantic' | 'analysis'>(
		($page.url.searchParams.get('tab') as 'info' | 'chapters' | 'entities' | 'annotations' | 'similar' | 'characters' | 'semantic' | 'analysis') || 'info'
	);
	let showEditModal = $state(false);
		let editForm = $state({ title: '', author: '', description: '', reading_status: 'unread' as ReadingStatus, language: '', genres: '' as string, tags: '' as string });
	let rating = $state(0);
	let hoverRating = $state(0);
	let savingRating = $state(false);
	let chapterSearch = $state('');
	let chapterDisplayLimit = $state(50);
	let collectionDialogOpen = $state(false);
	let analysisSubmitting = $state(false);
	let analysisRefreshKey = $state(0);
	let exportingAnnotations = $state<AnnotationExportFormat | null>(null);
	let customFields = $state<Record<string, unknown>>({});
	let customFieldRows = $state<Array<{ key: string; value: string }>>([]);
	let savingCustomFields = $state(false);
	let titleSuggestionLoading = $state<number | null>(null);
	let titleSuggestions = $state<Record<number, ChapterTitleResult>>({});
	let plotChecking = $state(false);
	let plotReport = $state<PlotHoleReport | null>(null);
	let summarizingCommunities = $state(false);
	let communitySummary = $state<CommunitySummaryResult | null>(null);

	onMount(async () => {
		try {
			const [b, ch, an, fields] = await Promise.all([
				api.getBook(bookId),
				api.getChapters(bookId),
				api.getAnnotations(bookId),
				api.getBookCustomFields(bookId).catch(() => ({})),
			]);
			book = b;
			chapters = ch ?? [];
			annotations = an ?? [];
			customFields = fields ?? {};
			syncCustomFieldRows(customFields);
			editForm = {
				title: b.title,
				author: b.author ?? '',
				description: b.description ?? '',
					reading_status: b.reading_status,
				language: b.language ?? 'zh',
				genres: (b.genres ?? []).join(', '),
				tags: (b.tags ?? []).join(', '),
			};
			rating = b.rating ?? 0;

			// Load entities for this book
			try {
				entities = await api.getEntities({ book_id: bookId });
			} catch { /* entities may not be extracted yet */ }
		} catch (e: unknown) {
			toast.error(`加载失败: ${getErrorMessage(e)}`);
		} finally {
			loading = false;
		}
	});

	async function saveEdit() {
		if (!book) return;
		try {
					await api.updateBook(book.id, {
						title: editForm.title,
						author: editForm.author || undefined,
						description: editForm.description || undefined,
						language: editForm.language as Language,
						genres: editForm.genres.split(',').map(s => s.trim()).filter(Boolean),
						tags: editForm.tags.split(',').map(s => s.trim()).filter(Boolean),
						rating: rating > 0 ? rating : null,
					});
				await api.updateBookReadingStatus(book.id, editForm.reading_status);
			// Reload
			book = await api.getBook(bookId);
			showEditModal = false;
			toast.success('书籍信息已更新');
		} catch (e: unknown) {
			toast.error(getErrorMessage(e));
		}
	}

	async function setRating(value: number) {
		if (!book || savingRating) return;
		const previous = rating;
		rating = value;
		savingRating = true;
		try {
			const updated = await api.updateBook(book.id, { rating: value } as Partial<Book>);
			book = { ...book, ...updated, rating: value };
			toast.success('评分已保存');
		} catch (e: unknown) {
			rating = previous;
			toast.error(`评分保存失败: ${getErrorMessage(e)}`);
		} finally {
			savingRating = false;
		}
	}

	async function handleDelete() {
		if (!book || !confirm(`确定删除「${book.title}」？此操作不可撤销。`)) return;
		await api.deleteBook(book.id);
		toast.success('已删除');
		window.location.href = '/library';
	}

	async function handleReprocess() {
		if (!book) return;
		await api.reprocessBook(book.id);
		toast.success('重新处理任务已提交');
	}

	let batchProcessing = $state(false);
	async function handleBatchAI() {
		if (!book) return;
		batchProcessing = true;
		try {
			const result = await api.aiBatchProcess(book.id);
			toast.success(`AI 全书处理完成: 处理${result.chapters_processed}章, 发现${result.entities_found}个实体, 生成${result.tags_generated.length}个标签`);
			// Reload entities
			entities = await api.getEntities({ book_id: bookId });
			// Reload book for updated tags
			book = await api.getBook(bookId);
		} catch (e: unknown) {
			toast.error(`AI 处理失败: ${getErrorMessage(e)}`);
		} finally {
			batchProcessing = false;
		}
	}

	async function handleDeepAnalysis() {
		if (!book) return;
		analysisSubmitting = true;
		try {
			await api.submitPipeline(book.id, 'deep_analysis');
			analysisRefreshKey += 1;
			toast.success('深度分析任务已提交');
		} catch (e: unknown) {
			toast.error(`深度分析提交失败: ${getErrorMessage(e)}`);
		} finally {
			analysisSubmitting = false;
		}
	}

	async function handleSuggestChapterTitle(chapterIndex: number) {
		if (!book || titleSuggestionLoading !== null) return;
		titleSuggestionLoading = chapterIndex;
		try {
			const result = await api.generateChapterTitles(book.id, chapterIndex);
			titleSuggestions = { ...titleSuggestions, [chapterIndex]: result };
			toast.success('章节标题建议已生成');
		} catch (e: unknown) {
			toast.error(`标题建议失败: ${getErrorMessage(e)}`);
		} finally {
			titleSuggestionLoading = null;
		}
	}

	async function handlePlotHoleCheck() {
		if (!book || plotChecking) return;
		plotChecking = true;
		try {
			plotReport = await api.detectPlotHoles(book.id);
			toast.success('情节一致性诊断完成');
		} catch (e: unknown) {
			toast.error(`情节诊断失败: ${getErrorMessage(e)}`);
		} finally {
			plotChecking = false;
		}
	}

	async function handleSummarizeCommunities() {
		if (!book || summarizingCommunities) return;
		summarizingCommunities = true;
		try {
			communitySummary = await api.summarizeCommunities(book.id);
			if (communitySummary.status === 'no_unsummarized_communities') {
				toast.message('没有待摘要的图谱社群');
			} else {
				toast.success(`已摘要 ${communitySummary.summarized ?? 0} 个图谱社群`);
			}
		} catch (e: unknown) {
			toast.error(`社群摘要失败: ${getErrorMessage(e)}`);
		} finally {
			summarizingCommunities = false;
		}
	}

	function issueTone(severity: string): string {
		const normalized = severity.toLowerCase();
		if (normalized.includes('high') || normalized.includes('严重')) return 'border-red-500/30 bg-red-500/10 text-red-300';
		if (normalized.includes('medium') || normalized.includes('中')) return 'border-amber-500/30 bg-amber-500/10 text-amber-300';
		return 'border-ink-700/50 bg-ink-900/50 text-ink-300';
	}

	function safeDownloadName(value: string): string {
		return value.replace(/[\\/:*?"<>|]+/g, '_').trim() || 'annotations';
	}

	async function handleExportAnnotations(format: AnnotationExportFormat) {
		if (!book || exportingAnnotations) return;
		exportingAnnotations = format;
		try {
			const blob = await api.exportAnnotations(book.id, format);
			const url = URL.createObjectURL(blob);
			const link = document.createElement('a');
			link.href = url;
			link.download = `${safeDownloadName(book.title)}-annotations.${format === 'markdown' ? 'md' : 'json'}`;
			link.click();
			setTimeout(() => URL.revokeObjectURL(url), 0);
			toast.success('批注已导出');
		} catch (e: unknown) {
			toast.error(`批注导出失败: ${getErrorMessage(e)}`);
		} finally {
			exportingAnnotations = null;
		}
	}

	function formatFileSize(bytes?: number): string {
		if (!bytes) return '未知';
		const units = ['B', 'KB', 'MB', 'GB'];
		const i = Math.floor(Math.log(bytes) / Math.log(1024));
		return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`;
	}

	function readinessLabel(ready: boolean): string {
		return ready ? '已就绪' : '待生成';
	}

	function customFieldValue(value: unknown): string {
		if (value === null || value === undefined) return '';
		if (typeof value === 'object') return JSON.stringify(value);
		return String(value);
	}

	function syncCustomFieldRows(fields: Record<string, unknown>) {
		customFieldRows = Object.entries(fields)
			.filter(([, value]) => value !== null)
			.map(([key, value]) => ({
				key,
				value: customFieldValue(value),
			}));
	}

	function parseCustomFieldValue(value: string): unknown {
		const trimmed = value.trim();
		if (!trimmed) return '';
		if (/^(true|false|null)$/i.test(trimmed) || /^-?\d+(\.\d+)?$/.test(trimmed) || /^[\[{]/.test(trimmed)) {
			try {
				return JSON.parse(trimmed);
			} catch {
				return value;
			}
		}
		return value;
	}

	function addCustomField() {
		customFieldRows = [...customFieldRows, { key: '', value: '' }];
	}

	function removeCustomField(index: number) {
		customFieldRows = customFieldRows.filter((_, i) => i !== index);
	}

	async function saveCustomFields() {
		if (!book || savingCustomFields) return;
		const fields: Record<string, unknown> = {};
		for (const row of customFieldRows) {
			const key = row.key.trim();
			if (!key) continue;
			fields[key] = parseCustomFieldValue(row.value);
		}
		const nextKeys = new Set(Object.keys(fields));
		for (const key of Object.keys(customFields)) {
			if (!nextKeys.has(key)) fields[key] = null;
		}
		savingCustomFields = true;
		try {
			const updatedFields = await api.updateBookCustomFields(book.id, fields);
			customFields = Object.fromEntries(Object.entries(updatedFields).filter(([, value]) => value !== null));
			syncCustomFieldRows(customFields);
			toast.success('自定义字段已保存');
		} catch (e: unknown) {
			toast.error(`自定义字段保存失败: ${getErrorMessage(e)}`);
		} finally {
			savingCustomFields = false;
		}
	}
</script>

<svelte:head>
	<title>{book?.title ?? '加载中...'} — Nova Reader</title>
</svelte:head>

<div class="mx-auto max-w-[1600px] px-4 py-6 sm:px-6 lg:px-8 animate-fade-in">
	{#if loading}
		<div class="animate-pulse space-y-6">
			<div class="flex gap-8">
				<div class="h-80 w-56 rounded-2xl bg-ink-800/50"></div>
				<div class="flex-1 space-y-4">
					<div class="h-8 w-1/2 rounded bg-ink-800/50"></div>
					<div class="h-5 w-1/3 rounded bg-ink-800/50"></div>
					<div class="h-20 w-full rounded bg-ink-800/30"></div>
				</div>
			</div>
		</div>
	{:else if book}
		<!-- Book Header -->
		<div class="flex gap-8">
			<!-- Cover -->
			<div class="shrink-0">
				<div class="h-80 w-56 overflow-hidden rounded-2xl bg-ink-800/50 shadow-xl">
					{#if book.cover_path}
						<img src="/api/covers/{book.id}" alt={book.title} class="h-full w-full object-cover" />
					{:else}
						<div class="flex h-full flex-col items-center justify-center bg-gradient-to-br from-ink-800 to-ink-900 p-6 text-center">
							<span class="text-lg font-bold text-ink-300">{book.title}</span>
							{#if book.author}
								<span class="mt-3 text-sm text-ink-500">{book.author}</span>
							{/if}
						</div>
					{/if}
				</div>
			</div>

			<!-- Info -->
			<div class="flex-1 space-y-4">
				<div>
					<h1 class="text-3xl font-bold text-ink-50">{book.title}</h1>
					<p class="mt-1 text-lg text-ink-400">{book.author ?? '未知作者'}</p>
				</div>

				<!-- Tags & Status -->
				<div class="flex flex-wrap gap-2">
					<span class="rounded-md bg-accent-500/10 px-2.5 py-1 text-xs font-medium text-accent-400">
						{book.reading_status === 'reading' ? '在读' : book.reading_status === 'completed' ? '已读' : book.reading_status === 'on_hold' ? '搁置' : book.reading_status === 'dropped' ? '弃读' : '未读'}
					</span>
					<span class="rounded-md bg-ink-800 px-2.5 py-1 text-xs text-ink-300">
						{book.format.toUpperCase()}
					</span>
					<span class="rounded-md bg-ink-800 px-2.5 py-1 text-xs text-ink-300">
						{book.language === 'zh' ? '中文' : book.language === 'en' ? 'English' : book.language}
					</span>
					{#each book.tags as tag}
						<span class="rounded-md bg-ink-800 px-2.5 py-1 text-xs text-ink-400">{tag}</span>
					{/each}
				</div>

				<!-- Index Status Badges -->
				<div class="flex flex-wrap gap-2">
					<span class="inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-[10px] font-medium {book.status === 'ready' ? 'bg-emerald-500/10 text-emerald-400' : book.status === 'processing' ? 'bg-amber-500/10 text-amber-400' : 'bg-ink-800 text-ink-500'}">
						<span class="h-1.5 w-1.5 rounded-full {book.status === 'ready' ? 'bg-emerald-400' : book.status === 'processing' ? 'bg-amber-400 animate-pulse' : 'bg-ink-600'}"></span>
						搜索 {book.status === 'ready' ? '已索引' : book.status === 'processing' ? '索引中' : '未索引'}
					</span>
					<span class="inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-[10px] font-medium {entities.length > 0 ? 'bg-purple-500/10 text-purple-400' : 'bg-ink-800 text-ink-500'}">
						<span class="h-1.5 w-1.5 rounded-full {entities.length > 0 ? 'bg-purple-400' : 'bg-ink-600'}"></span>
						图谱 {entities.length > 0 ? `${entities.length} 实体` : '未提取'}
					</span>
					<span class="inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-[10px] font-medium {book.status === 'ready' ? 'bg-blue-500/10 text-blue-400' : 'bg-ink-800 text-ink-500'}">
						<span class="h-1.5 w-1.5 rounded-full {book.status === 'ready' ? 'bg-blue-400' : 'bg-ink-600'}"></span>
						向量 {book.status === 'ready' ? '已嵌入' : '未嵌入'}
					</span>
				</div>

				<!-- Stats -->
				<div class="flex gap-6 text-sm text-ink-400">
					{#if book.word_count}
						<span>{Math.round(book.word_count / 10000)}万字</span>
						<span title="按 500字/分钟估算">约 {Math.round(book.word_count / 500 / 60)}小时</span>
					{/if}
					<span>{book.chapter_count} 章</span>
					<span>进度 {Math.round(book.progress * 100)}%</span>
				</div>

				<!-- Progress bar -->
				<div class="h-2 w-full max-w-md overflow-hidden rounded-full bg-ink-800">
					<div class="h-full rounded-full bg-accent-500" style="width: {book.progress * 100}%"></div>
				</div>

				<!-- Reading Status Selector -->
				<div class="flex items-center gap-4">
					<div class="flex items-center gap-3">
						<span class="text-xs text-ink-500">阅读状态</span>
						<select
							value={book.reading_status}
							onchange={async (e) => {
								const newStatus = (e.target as HTMLSelectElement).value;
								try {
									await api.updateBookReadingStatus(book!.id, newStatus as ReadingStatus);
								book = { ...book!, reading_status: newStatus as ReadingStatus };
								toast.success('阅读状态已更新');
							} catch (err: unknown) {
								toast.error(getErrorMessage(err));
								}
							}}
							class="rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-1.5 text-sm text-ink-200 outline-none focus:border-accent-500/30"
						>
							<option value="unread">未读</option>
							<option value="reading">在读</option>
							<option value="completed">已读</option>
							<option value="on_hold">搁置</option>
							<option value="dropped">弃读</option>
						</select>
					</div>

					<!-- Star Rating (inline) -->
					<div class="flex items-center gap-2">
						<span class="text-xs text-ink-500">评分:</span>
						<div class="flex gap-0.5">
								{#each Array(5) as _, i}
									<button
										onclick={() => setRating(i + 1)}
										onmouseenter={() => hoverRating = i + 1}
										onmouseleave={() => hoverRating = 0}
										class="text-lg transition-colors"
										class:text-amber-400={(hoverRating || rating) > i}
										class:text-ink-600={(hoverRating || rating) <= i}
										disabled={savingRating}
										aria-label={`评分 ${i + 1} 星`}
									>★</button>
								{/each}
							</div>
						{#if rating > 0}
							<span class="text-xs text-ink-400">{rating}/5</span>
						{/if}
					</div>
				</div>

				<!-- Description -->
				{#if book.description}
					<p class="text-sm text-ink-300 leading-relaxed max-w-prose">{book.description}</p>
				{/if}

				<!-- Actions -->
				<div class="flex gap-3 pt-2">
					<a
						href="/reading/{book.id}"
						class="inline-flex items-center gap-2 rounded-lg bg-accent-500 px-5 py-2.5 text-sm font-medium text-ink-950 hover:bg-accent-400 transition-colors"
					>
						<BookOpen size={16} strokeWidth={2} />
						{book.progress > 0 ? '继续阅读' : '开始阅读'}
					</a>
					<button
						onclick={handleReprocess}
						class="inline-flex items-center gap-2 rounded-lg border border-ink-700/50 bg-ink-900/50 px-4 py-2.5 text-sm text-ink-200 hover:border-ink-600 transition-colors"
					>
						<RefreshCw size={16} strokeWidth={2} />
						重新处理
					</button>
					<button
						onclick={handleBatchAI}
						disabled={batchProcessing}
						class="inline-flex items-center gap-2 rounded-lg border border-purple-500/30 bg-purple-500/10 px-4 py-2.5 text-sm text-purple-300 hover:bg-purple-500/20 hover:border-purple-500/50 transition-colors disabled:opacity-50"
					>
						<Lightbulb size={16} strokeWidth={2} />
						{batchProcessing ? 'AI 处理中...' : 'AI 全书分析'}
					</button>
					<button
						onclick={() => showEditModal = true}
						class="inline-flex items-center gap-2 rounded-lg border border-ink-700/50 bg-ink-900/50 px-4 py-2.5 text-sm text-ink-200 hover:border-ink-600 transition-colors"
					>
						<PenSquare size={16} strokeWidth={2} />
						编辑信息
					</button>
					<button
						onclick={() => collectionDialogOpen = true}
						class="inline-flex items-center gap-2 rounded-lg border border-ink-700/50 bg-ink-900/50 px-4 py-2.5 text-sm text-ink-200 hover:border-ink-600 transition-colors"
					>
						<FolderPlus size={16} strokeWidth={2} />
						添加到合集
					</button>
					<button
						onclick={() => {
							const a = document.createElement('a');
							a.href = `/api/books/${book!.id}/download`;
							a.download = '';
							a.click();
						}}
						class="inline-flex items-center gap-2 rounded-lg border border-ink-700/50 bg-ink-900/50 px-4 py-2.5 text-sm text-ink-200 hover:border-ink-600 transition-colors"
					>
						<Download size={16} strokeWidth={2} />
						下载
					</button>
					<button
						onclick={handleDelete}
						class="inline-flex items-center gap-2 rounded-lg border border-error/30 bg-error/5 px-4 py-2.5 text-sm text-error hover:bg-error/10 transition-colors"
					>
						<Trash2 size={16} strokeWidth={2} />
						删除
					</button>
				</div>

				<!-- AI Analysis Progress Indicator -->
				<div class="mt-4 rounded-lg border border-ink-800/50 bg-ink-900/30 p-3">
					<p class="text-xs font-medium text-ink-300 mb-2">AI 分析状态</p>
					<div class="grid grid-cols-3 gap-3">
						<div class="text-center">
							<div class="mx-auto mb-1 flex h-8 w-8 items-center justify-center rounded-full {entities.length > 0 ? 'bg-green-500/15 text-green-400' : 'bg-ink-800 text-ink-500'}">
								{#if entities.length > 0}✓{:else}—{/if}
							</div>
							<p class="text-[10px] text-ink-400">实体提取</p>
							<p class="text-[10px] text-ink-500">{entities.length > 0 ? `${entities.length} 个` : '未执行'}</p>
						</div>
						<div class="text-center">
							<div class="mx-auto mb-1 flex h-8 w-8 items-center justify-center rounded-full {book.description ? 'bg-green-500/15 text-green-400' : 'bg-ink-800 text-ink-500'}">
								{#if book.description}✓{:else}—{/if}
							</div>
							<p class="text-[10px] text-ink-400">摘要生成</p>
							<p class="text-[10px] text-ink-500">{book.description ? '已完成' : '未执行'}</p>
						</div>
						<div class="text-center">
							<div class="mx-auto mb-1 flex h-8 w-8 items-center justify-center rounded-full {book.tags && book.tags.length > 0 ? 'bg-green-500/15 text-green-400' : 'bg-ink-800 text-ink-500'}">
								{#if book.tags && book.tags.length > 0}✓{:else}—{/if}
							</div>
							<p class="text-[10px] text-ink-400">标签分析</p>
							<p class="text-[10px] text-ink-500">{book.tags?.length ? `${book.tags.length} 个` : '未执行'}</p>
						</div>
					</div>
				</div>
			</div>
		</div>

		<!-- Tabs -->
		<div class="mt-8 border-b border-ink-800/50">
			<nav class="flex gap-6">
				{#each [
					{ id: 'info', label: '详细信息' },
					{ id: 'chapters', label: `目录 (${chapters?.length ?? 0})` },
					{ id: 'characters', label: '人物' },
					{ id: 'entities', label: `知识实体 (${entities?.length ?? 0})` },
					{ id: 'annotations', label: `批注 (${annotations?.length ?? 0})` },
					{ id: 'similar', label: '相似推荐' },
					{ id: 'semantic', label: '智能标签' },
					{ id: 'analysis', label: '深度分析' },
				] as tab}
					<button
						onclick={() => {
							activeTab = tab.id as typeof activeTab;
							const url = new URL($page.url);
							if (tab.id === 'info') url.searchParams.delete('tab');
							else url.searchParams.set('tab', tab.id);
							goto(url.toString(), { replaceState: true, noScroll: true });
						}}
						class="border-b-2 pb-3 text-sm font-medium transition-colors"
						class:border-accent-500={activeTab === tab.id}
						class:text-accent-400={activeTab === tab.id}
						class:border-transparent={activeTab !== tab.id}
						class:text-ink-400={activeTab !== tab.id}
						class:hover:text-ink-200={activeTab !== tab.id}
					>
						{tab.label}
					</button>
				{/each}
			</nav>
		</div>

		<!-- Tab content -->
		<div class="mt-6">
			{#if activeTab === 'info'}
				<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
					<div class="rounded-lg border border-ink-800/50 bg-ink-900/30 p-4">
						<div class="text-xs text-ink-500 mb-1">文件路径</div>
						<div class="text-sm text-ink-300 font-mono truncate">{book.file_path}</div>
					</div>
					<div class="rounded-lg border border-ink-800/50 bg-ink-900/30 p-4">
						<div class="text-xs text-ink-500 mb-1">文件哈希</div>
						<div class="text-sm text-ink-300 font-mono truncate">{book.file_hash}</div>
					</div>
					{#if book.file_size_bytes}
						<div class="rounded-lg border border-ink-800/50 bg-ink-900/30 p-4">
							<div class="text-xs text-ink-500 mb-1">文件大小</div>
							<div class="text-sm text-ink-300">{book.file_size_bytes >= 1048576 ? `${(book.file_size_bytes / 1048576).toFixed(1)} MB` : `${(book.file_size_bytes / 1024).toFixed(0)} KB`}</div>
						</div>
					{/if}
					{#if book.format}
						<div class="rounded-lg border border-ink-800/50 bg-ink-900/30 p-4">
							<div class="text-xs text-ink-500 mb-1">格式</div>
							<div class="text-sm text-ink-300 uppercase">{book.format}</div>
						</div>
					{/if}
					{#if book.language}
						<div class="rounded-lg border border-ink-800/50 bg-ink-900/30 p-4">
							<div class="text-xs text-ink-500 mb-1">语言</div>
							<div class="text-sm text-ink-300">{book.language}</div>
						</div>
					{/if}
					{#if book.word_count}
						<div class="rounded-lg border border-ink-800/50 bg-ink-900/30 p-4">
							<div class="text-xs text-ink-500 mb-1">字数</div>
							<div class="text-sm text-ink-300">{book.word_count >= 10000 ? `${(book.word_count / 10000).toFixed(1)}万字` : `${book.word_count}字`}</div>
						</div>
					{/if}
					<div class="rounded-lg border border-ink-800/50 bg-ink-900/30 p-4">
						<div class="text-xs text-ink-500 mb-1">添加时间</div>
						<div class="text-sm text-ink-300">{new Date(book.created_at).toLocaleString('zh-CN')}</div>
					</div>
					{#if book.series}
						<div class="rounded-lg border border-ink-800/50 bg-ink-900/30 p-4">
							<div class="text-xs text-ink-500 mb-1">系列</div>
							<div class="text-sm text-ink-300">{book.series} #{book.series_index}</div>
						</div>
					{/if}
				</div>

				<section class="mt-6 rounded-xl border border-ink-800/60 bg-ink-900/30 p-5" aria-label="书籍自定义字段">
					<div class="flex flex-wrap items-center justify-between gap-3">
						<div>
							<h3 class="text-sm font-semibold text-ink-100">自定义字段</h3>
							<p class="mt-1 text-xs text-ink-500">记录来源、版本、阅读备注等不属于标准元数据的信息。</p>
						</div>
						<div class="flex items-center gap-2">
							<button
								type="button"
								onclick={addCustomField}
								class="inline-flex items-center gap-1.5 rounded-lg border border-ink-700/50 px-3 py-1.5 text-xs text-ink-300 transition-colors hover:border-accent-500/30 hover:text-accent-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
							>
								<Plus size={13} />
								添加字段
							</button>
							<button
								type="button"
								onclick={saveCustomFields}
								disabled={savingCustomFields}
								class="inline-flex items-center gap-1.5 rounded-lg bg-accent-500 px-3 py-1.5 text-xs font-medium text-ink-950 transition-colors hover:bg-accent-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-300/80 disabled:cursor-not-allowed disabled:opacity-50"
							>
								<Check size={13} />
								{savingCustomFields ? '保存中…' : '保存字段'}
							</button>
						</div>
					</div>

					{#if customFieldRows.length === 0}
						<div class="mt-4 rounded-lg border border-dashed border-ink-800/70 bg-ink-950/20 p-5 text-center">
							<p class="text-sm text-ink-500">暂无自定义字段</p>
						</div>
					{:else}
						<div class="mt-4 space-y-2">
							{#each customFieldRows as row, i}
								<div class="grid gap-2 rounded-lg border border-ink-800/50 bg-ink-950/30 p-3 sm:grid-cols-[minmax(0,220px)_minmax(0,1fr)_36px]">
									<div>
										<label for={`custom-field-key-${i}`} class="sr-only">字段名</label>
										<input
											id={`custom-field-key-${i}`}
											name={`custom-field-key-${i}`}
											autocomplete="off"
											bind:value={row.key}
											placeholder="字段名…"
											class="w-full rounded-lg border border-ink-700/50 bg-ink-900/60 px-3 py-2 text-sm text-ink-200 placeholder:text-ink-600 focus:border-accent-500/50 focus:outline-none"
										/>
									</div>
									<div>
										<label for={`custom-field-value-${i}`} class="sr-only">字段值</label>
										<input
											id={`custom-field-value-${i}`}
											name={`custom-field-value-${i}`}
											autocomplete="off"
											bind:value={row.value}
											placeholder="字段值…"
											class="w-full rounded-lg border border-ink-700/50 bg-ink-900/60 px-3 py-2 text-sm text-ink-200 placeholder:text-ink-600 focus:border-accent-500/50 focus:outline-none"
										/>
									</div>
									<button
										type="button"
										onclick={() => removeCustomField(i)}
										aria-label={`移除自定义字段 ${row.key || i + 1}`}
										class="grid h-9 w-9 place-items-center rounded-lg text-ink-500 transition-colors hover:bg-red-500/10 hover:text-red-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red-500/50"
									>
										<X size={14} />
									</button>
								</div>
							{/each}
						</div>
					{/if}
				</section>

			{:else if activeTab === 'chapters'}
				{#if chapters.length > 20}
					<div class="mb-3">
						<input
							type="text"
							placeholder="搜索章节标题..."
							class="w-full rounded-lg border border-ink-700/40 bg-ink-900/50 px-3 py-2 text-sm text-ink-200 placeholder:text-ink-500 focus:border-amber-500/50 focus:outline-none"
							bind:value={chapterSearch}
						/>
					</div>
				{/if}
				{@const filteredChapters = chapterSearch
					? chapters.map((c, i) => ({ ...c, _idx: i })).filter(c => c.title.includes(chapterSearch))
					: chapters.map((c, i) => ({ ...c, _idx: i }))}
				{@const displayChapters = filteredChapters.slice(0, chapterDisplayLimit)}
				<div class="space-y-1">
					{#each displayChapters as chapter}
						{@const aiChapterIndex = chapter.index ?? chapter._idx}
						<div class="rounded-lg transition-colors hover:bg-ink-800/30">
							<div class="flex items-center gap-3 px-4 py-3">
								<a
									href="/reading/{book.id}?chapter={chapter._idx}"
									class="flex min-w-0 flex-1 items-center gap-4"
								>
									<span class="w-8 shrink-0 text-right text-sm text-ink-500">{chapter._idx + 1}</span>
									<span class="min-w-0 flex-1 truncate text-sm text-ink-200">{chapter.title}</span>
									<span class="shrink-0 text-xs text-ink-500">{Math.round(chapter.word_count / 1000)}k字</span>
								</a>
								<button
									type="button"
									onclick={() => handleSuggestChapterTitle(aiChapterIndex)}
									disabled={titleSuggestionLoading !== null}
									aria-label={`为第 ${chapter._idx + 1} 章生成标题建议`}
									class="grid h-8 w-8 shrink-0 place-items-center rounded-lg text-ink-500 transition-colors hover:bg-accent-500/10 hover:text-accent-300 disabled:cursor-not-allowed disabled:opacity-40"
								>
									{#if titleSuggestionLoading === aiChapterIndex}
										<div class="h-3.5 w-3.5 animate-spin rounded-full border-2 border-accent-400 border-t-transparent"></div>
									{:else}
										<Sparkles size={14} />
									{/if}
								</button>
							</div>
							{#if titleSuggestions[aiChapterIndex]}
								<div class="border-t border-ink-800/40 px-16 pb-3 pt-2">
									<div class="flex flex-wrap gap-2">
										{#each titleSuggestions[aiChapterIndex].titles as suggestion, i}
											<span class="rounded-md border border-accent-500/20 bg-accent-500/10 px-2 py-1 text-xs text-accent-200">
												{suggestion.title}
												{#if suggestion.style}
													<span class="ml-1 text-accent-400/70">/{suggestion.style}</span>
												{/if}
												{#if i === titleSuggestions[aiChapterIndex].recommended}
													<span class="ml-1 text-emerald-300">推荐</span>
												{/if}
											</span>
										{/each}
									</div>
								</div>
							{/if}
						</div>
					{/each}
					{#if filteredChapters.length > chapterDisplayLimit}
						<button
							class="w-full rounded-lg px-4 py-3 text-sm text-ink-400 hover:text-amber-400 hover:bg-ink-800/30 transition-colors"
							onclick={() => chapterDisplayLimit += 100}
						>
							显示更多 ({filteredChapters.length - chapterDisplayLimit} 章待展示)
						</button>
					{/if}
				</div>

			{:else if activeTab === 'entities'}
				<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
					{#each entities as entity}
						<a
							href="/characters/{entity.id}"
							class="flex items-start gap-3 rounded-lg border border-ink-800/50 bg-ink-900/30 p-4 hover:border-ink-700/50 transition-colors"
						>
							<span
								class="mt-0.5 h-3 w-3 shrink-0 rounded-full"
								class:bg-amber-400={entity.type === 'person'}
								class:bg-emerald-400={entity.type === 'location'}
								class:bg-indigo-400={entity.type === 'organization'}
								class:bg-pink-400={entity.type === 'item'}
								class:bg-violet-400={entity.type === 'concept'}
							></span>
							<div>
								<div class="font-medium text-ink-200">{entity.name}</div>
								<div class="mt-1 text-xs text-ink-500 line-clamp-2">{entity.description}</div>
							</div>
						</a>
					{/each}
				</div>

			{:else if activeTab === 'annotations'}
				<div class="mb-4 flex flex-wrap items-center justify-between gap-3">
					<div>
						<h3 class="text-lg font-medium text-ink-100">批注</h3>
						<p class="mt-1 text-sm text-ink-500">{annotations.length} 条高亮和笔记</p>
					</div>
					<div class="flex flex-wrap items-center gap-2">
						<button
							type="button"
							onclick={() => handleExportAnnotations('markdown')}
							disabled={annotations.length === 0 || exportingAnnotations !== null}
							aria-label="导出 Markdown 批注"
							class="inline-flex items-center gap-2 rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-2 text-sm text-ink-200 transition-colors hover:border-ink-600 disabled:cursor-not-allowed disabled:opacity-50"
						>
							<Download size={14} />
							Markdown
						</button>
						<button
							type="button"
							onclick={() => handleExportAnnotations('json')}
							disabled={annotations.length === 0 || exportingAnnotations !== null}
							aria-label="导出 JSON 批注"
							class="inline-flex items-center gap-2 rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-2 text-sm text-ink-200 transition-colors hover:border-ink-600 disabled:cursor-not-allowed disabled:opacity-50"
						>
							<Download size={14} />
							JSON
						</button>
						<button
							type="button"
							onclick={() => handleExportAnnotations('notion')}
							disabled={annotations.length === 0 || exportingAnnotations !== null}
							aria-label="导出 Notion 批注"
							class="inline-flex items-center gap-2 rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-2 text-sm text-ink-200 transition-colors hover:border-ink-600 disabled:cursor-not-allowed disabled:opacity-50"
						>
							<Download size={14} />
							Notion
						</button>
					</div>
				</div>
				{#if annotations.length === 0}
					<div class="py-12 text-center text-ink-500">暂无批注，阅读时选中文本即可添加</div>
				{:else}
					<div class="space-y-3">
						{#each annotations as ann}
							<div class="rounded-lg border border-ink-800/50 bg-ink-900/30 p-4">
								<div class="flex items-center gap-2 mb-2">
									<div class="h-2.5 w-2.5 rounded-full" style="background: {ann.color}"></div>
									<span class="text-xs text-ink-500">第 {ann.chapter_index + 1} 章</span>
								</div>
								<blockquote class="border-l-2 border-ink-700 pl-3 text-sm text-ink-300 italic">
									"{ann.selected_text}"
								</blockquote>
								{#if ann.note}
									<p class="mt-2 text-sm text-ink-400">{ann.note}</p>
								{/if}
							</div>
						{/each}
					</div>
				{/if}

			{:else if activeTab === 'characters'}
				<!-- Characters / Relationship Graph for this book -->
				<div class="space-y-4">
					<p class="text-sm text-ink-400">本书角色关系图谱 — 通过 AI 分析提取的人物及其关系</p>
					{#if entities.filter(e => e.type === 'person').length === 0}
						<div class="py-12 text-center">
							<p class="text-ink-500 mb-3">尚未提取人物信息</p>
							<button
								onclick={handleBatchAI}
								disabled={batchProcessing}
								class="inline-flex items-center gap-2 rounded-lg bg-accent-500 px-4 py-2 text-sm font-medium text-ink-950 hover:bg-accent-400 disabled:opacity-50 transition-colors"
							>
								<Lightbulb size={14} />
								运行 AI 人物提取
							</button>
						</div>
					{:else}
						<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
							{#each entities.filter(e => e.type === 'person') as character}
								<a
									href="/characters/{character.id}"
									class="rounded-lg border border-ink-800/50 bg-ink-900/30 p-4 hover:border-amber-500/20 transition-colors"
								>
									<div class="flex items-center gap-3">
										<div class="h-10 w-10 rounded-full bg-amber-500/10 flex items-center justify-center text-amber-400 font-bold text-sm">
											{character.name.slice(0, 1)}
										</div>
										<div>
											<div class="text-sm font-medium text-ink-100">{character.name}</div>
											<div class="text-xs text-ink-500 line-clamp-1">{character.description || '暂无描述'}</div>
										</div>
									</div>
								</a>
							{/each}
						</div>
					{/if}
				</div>

			{:else if activeTab === 'similar'}
				<!-- Similar Books Recommendations -->
				<SimilarBooks bookId={bookId} />

			{:else if activeTab === 'semantic'}
				<!-- Semantic Intelligence -->
				<div class="space-y-6">
					<div class="flex items-center gap-3">
						<h3 class="text-lg font-medium text-ink-100">智能标签分析</h3>
						<button
							onclick={async () => { await api.computeBookTags(bookId); toast.success('智能标签计算任务已提交'); }}
							class="inline-flex items-center gap-2 rounded-lg border border-accent-500/20 bg-accent-500/10 px-3 py-1.5 text-xs text-accent-400 hover:bg-accent-500/20 transition-colors"
						>
							计算智能标签
						</button>
					</div>
					<div class="grid gap-6 lg:grid-cols-2">
						<div class="rounded-xl border border-ink-700/50 bg-ink-900/30 p-5">
							<h4 class="text-sm font-medium text-ink-300 mb-4">标签雷达</h4>
							<SemanticRadar {bookId} />
						</div>
						<div class="rounded-xl border border-ink-700/50 bg-ink-900/30 p-5">
							<h4 class="text-sm font-medium text-ink-300 mb-4">章节热力图</h4>
							<SemanticHeatmap {bookId} />
						</div>
					</div>
					<SemanticMarkersPanel {bookId} />
				</div>

			{:else if activeTab === 'analysis'}
				<div class="space-y-6">
					<div class="flex flex-wrap items-center justify-between gap-3">
						<div>
							<h3 class="text-lg font-medium text-ink-100">书籍级深度分析</h3>
							<p class="mt-1 text-sm text-ink-500">围绕当前书籍聚合结构、人物、标签、语义索引与阅读器上下文。</p>
						</div>
						<button
							onclick={handleDeepAnalysis}
							disabled={analysisSubmitting}
							class="inline-flex items-center gap-2 rounded-lg bg-accent-500 px-4 py-2 text-sm font-medium text-ink-950 transition-colors hover:bg-accent-400 disabled:opacity-50"
						>
							<Brain size={15} />
							{analysisSubmitting ? '提交中...' : '提交深度分析'}
						</button>
					</div>

					<div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
						{#each [
							{ label: '章节结构', value: `${chapters.length} 章`, ready: chapters.length > 0, icon: BookOpen },
							{ label: '人物实体', value: `${entities.length} 个`, ready: entities.length > 0, icon: Network },
							{ label: '智能标签', value: `${book.tags?.length ?? 0} 个`, ready: !!book.tags?.length, icon: Sparkles },
							{ label: '语义索引', value: book.status === 'ready' ? '可检索' : '待处理', ready: book.status === 'ready', icon: Search },
						] as item}
							{@const Icon = item.icon}
							<div class="rounded-xl border border-ink-800/60 bg-ink-900/35 p-4">
								<div class="flex items-center justify-between gap-3">
									<div class="flex h-9 w-9 items-center justify-center rounded-lg bg-ink-950/50 text-accent-300">
										<Icon size={17} />
									</div>
									<span class="rounded-full px-2 py-0.5 text-[10px] {item.ready ? 'bg-emerald-500/10 text-emerald-400' : 'bg-ink-800 text-ink-500'}">{readinessLabel(item.ready)}</span>
								</div>
								<p class="mt-3 text-xs text-ink-500">{item.label}</p>
								<p class="mt-1 text-lg font-semibold text-ink-100">{item.value}</p>
							</div>
						{/each}
					</div>

					<div class="grid gap-4 lg:grid-cols-[minmax(0,1fr)_320px]">
						<div class="rounded-xl border border-ink-800/60 bg-ink-900/30 p-5">
							<h4 class="flex items-center gap-2 text-sm font-medium text-ink-200"><BarChart3 size={16} /> 分析工作流</h4>
							<div class="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-5">
								<button onclick={handleBatchAI} disabled={batchProcessing} class="rounded-lg border border-purple-500/25 bg-purple-500/10 p-3 text-left transition-colors hover:bg-purple-500/15 disabled:opacity-50">
									<Lightbulb size={16} class="mb-2 text-purple-300" />
									<span class="block text-sm font-medium text-ink-100">全书处理</span>
									<span class="mt-1 block text-xs text-ink-500">摘要、实体、标签、风格</span>
								</button>
								<button onclick={async () => { await api.computeBookTags(bookId); toast.success('智能标签计算任务已提交'); }} class="rounded-lg border border-emerald-500/25 bg-emerald-500/10 p-3 text-left transition-colors hover:bg-emerald-500/15">
									<Sparkles size={16} class="mb-2 text-emerald-300" />
									<span class="block text-sm font-medium text-ink-100">智能标签</span>
									<span class="mt-1 block text-xs text-ink-500">生成书籍画像和章节热力图</span>
								</button>
								<button onclick={async () => { await api.aiIngestEmbeddings(bookId); toast.success('语义索引任务已提交'); }} class="rounded-lg border border-cyan-500/25 bg-cyan-500/10 p-3 text-left transition-colors hover:bg-cyan-500/15">
									<Network size={16} class="mb-2 text-cyan-300" />
									<span class="block text-sm font-medium text-ink-100">语义索引</span>
									<span class="mt-1 block text-xs text-ink-500">写入向量库用于 RAG 和推荐</span>
								</button>
								<button onclick={handlePlotHoleCheck} disabled={plotChecking} class="rounded-lg border border-red-500/25 bg-red-500/10 p-3 text-left transition-colors hover:bg-red-500/15 disabled:opacity-50">
									<AlertTriangle size={16} class="mb-2 text-red-300" />
									<span class="block text-sm font-medium text-ink-100">情节诊断</span>
									<span class="mt-1 block text-xs text-ink-500">{plotChecking ? '分析中...' : '检查摘要中的矛盾和断裂'}</span>
								</button>
								<button onclick={handleSummarizeCommunities} disabled={summarizingCommunities} class="rounded-lg border border-sky-500/25 bg-sky-500/10 p-3 text-left transition-colors hover:bg-sky-500/15 disabled:opacity-50">
									<Network size={16} class="mb-2 text-sky-300" />
									<span class="block text-sm font-medium text-ink-100">社群摘要</span>
									<span class="mt-1 block text-xs text-ink-500">{summarizingCommunities ? '生成中...' : '总结图谱社群关系'}</span>
								</button>
							</div>
						</div>

						<div class="rounded-xl border border-ink-800/60 bg-ink-900/30 p-5">
							<h4 class="text-sm font-medium text-ink-200">继续探索</h4>
							<div class="mt-4 space-y-2">
								<a href="/reading/{book.id}" class="flex items-center justify-between rounded-lg bg-ink-950/40 px-3 py-2 text-sm text-ink-300 transition-colors hover:text-accent-300"><span>进入阅读器工作台</span><BookOpen size={14} /></a>
								<a href="/search?q={encodeURIComponent(book.title)}&mode=hybrid" class="flex items-center justify-between rounded-lg bg-ink-950/40 px-3 py-2 text-sm text-ink-300 transition-colors hover:text-accent-300"><span>用本书发起搜索</span><Search size={14} /></a>
								<a href="/library/{book.id}?tab=similar" class="flex items-center justify-between rounded-lg bg-ink-950/40 px-3 py-2 text-sm text-ink-300 transition-colors hover:text-accent-300"><span>查看相似推荐</span><Sparkles size={14} /></a>
							</div>
						</div>
					</div>

					{#if plotReport || communitySummary}
						<div class="grid gap-4 lg:grid-cols-2">
							{#if plotReport}
								<section class="rounded-xl border border-ink-800/60 bg-ink-900/30 p-5" aria-label="情节一致性诊断">
									<div class="flex items-start justify-between gap-3">
										<div>
											<h4 class="text-sm font-medium text-ink-100">情节一致性诊断</h4>
											<p class="mt-1 text-xs text-ink-500">{plotReport.summary}</p>
										</div>
										<div class="rounded-lg bg-ink-950/50 px-3 py-2 text-right">
											<p class="text-[10px] text-ink-500">一致性</p>
											<p class="text-lg font-semibold text-accent-300">{plotReport.consistency_score}</p>
										</div>
									</div>
									{#if plotReport.issues.length === 0}
										<p class="mt-4 rounded-lg border border-emerald-500/20 bg-emerald-500/10 px-3 py-2 text-sm text-emerald-300">未发现明显情节漏洞。</p>
									{:else}
										<div class="mt-4 space-y-3">
											{#each plotReport.issues as issue}
												<div class="rounded-lg border border-ink-800/50 bg-ink-950/30 p-3">
													<div class="flex flex-wrap items-center gap-2">
														<span class={`rounded-md border px-2 py-0.5 text-[10px] ${issueTone(issue.severity)}`}>{issue.severity}</span>
														<span class="text-xs text-ink-400">{issue.type}</span>
														{#if issue.chapters.length}
															<span class="text-[10px] text-ink-500">第 {issue.chapters.join('、')} 章</span>
														{/if}
													</div>
													<p class="mt-2 text-sm text-ink-200">{issue.description}</p>
													{#if issue.entities.length}
														<p class="mt-1 text-xs text-ink-500">相关实体：{issue.entities.join('、')}</p>
													{/if}
													{#if issue.suggestion}
														<p class="mt-2 text-xs text-accent-300">建议：{issue.suggestion}</p>
													{/if}
												</div>
											{/each}
										</div>
									{/if}
								</section>
							{/if}

							{#if communitySummary}
								<section class="rounded-xl border border-ink-800/60 bg-ink-900/30 p-5" aria-label="图谱社群摘要">
									<h4 class="text-sm font-medium text-ink-100">图谱社群摘要</h4>
									<div class="mt-4 grid grid-cols-2 gap-3">
										<div class="rounded-lg bg-ink-950/40 p-3">
											<p class="text-[10px] text-ink-500">状态</p>
											<p class="mt-1 text-sm text-ink-200">{communitySummary.status}</p>
										</div>
										<div class="rounded-lg bg-ink-950/40 p-3">
											<p class="text-[10px] text-ink-500">本次摘要</p>
											<p class="mt-1 text-sm text-accent-300">{communitySummary.summarized ?? 0} / {communitySummary.total_communities ?? 0}</p>
										</div>
									</div>
									{#if communitySummary.errors.length > 0}
										<div class="mt-4 space-y-2">
											{#each communitySummary.errors as error}
												<p class="rounded-lg border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-300">{error}</p>
											{/each}
										</div>
									{:else}
										<p class="mt-4 rounded-lg border border-emerald-500/20 bg-emerald-500/10 px-3 py-2 text-sm text-emerald-300">社群摘要任务没有返回错误。</p>
									{/if}
								</section>
							{/if}
						</div>
					{/if}

					<!-- Real deep-analysis results -->
					<BookAnalysisPanel {bookId} chapterCount={chapters.length} refreshKey={analysisRefreshKey} />
				</div>
			{/if}
		</div>
	{/if}
</div>

<!-- Edit Modal -->
{#if showEditModal && book}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
		<button
			type="button"
			class="absolute inset-0 cursor-default"
			aria-label="关闭编辑书籍信息"
			onclick={() => showEditModal = false}
		></button>
		<div
			role="dialog"
			aria-modal="true"
			aria-labelledby="book-edit-title"
			tabindex="-1"
			class="relative w-full max-w-lg mx-4 rounded-2xl border border-ink-700/50 bg-ink-900 p-6 shadow-2xl"
		>
			<h2 id="book-edit-title" class="text-lg font-bold text-ink-100 mb-5">编辑书籍信息</h2>
			<div class="space-y-4">
				<div>
					<label for="book-edit-title-input" class="text-xs font-medium text-ink-400 mb-1 block">书名</label>
					<input id="book-edit-title-input" bind:value={editForm.title} class="w-full rounded-lg border border-ink-700/50 bg-ink-800/50 px-3 py-2 text-sm text-ink-100 outline-none focus:border-accent-500/30" />
				</div>
				<div>
					<label for="book-edit-author-input" class="text-xs font-medium text-ink-400 mb-1 block">作者</label>
					<input id="book-edit-author-input" bind:value={editForm.author} class="w-full rounded-lg border border-ink-700/50 bg-ink-800/50 px-3 py-2 text-sm text-ink-100 outline-none focus:border-accent-500/30" />
				</div>
				<div>
					<label for="book-edit-description-input" class="text-xs font-medium text-ink-400 mb-1 block">简介</label>
					<textarea id="book-edit-description-input" bind:value={editForm.description} rows="3" class="w-full rounded-lg border border-ink-700/50 bg-ink-800/50 px-3 py-2 text-sm text-ink-100 outline-none focus:border-accent-500/30 resize-none"></textarea>
				</div>
				<div class="grid grid-cols-2 gap-4">
					<div>
							<label for="book-edit-reading-status-input" class="text-xs font-medium text-ink-400 mb-1 block">阅读状态</label>
							<select id="book-edit-reading-status-input" bind:value={editForm.reading_status} class="w-full rounded-lg border border-ink-700/50 bg-ink-800/50 px-3 py-2 text-sm text-ink-100 outline-none focus:border-accent-500/30">
							<option value="unread">未读</option>
							<option value="reading">在读</option>
							<option value="completed">已读</option>
							<option value="on_hold">搁置</option>
							<option value="dropped">弃读</option>
						</select>
					</div>
					<div>
						<label for="book-edit-language-input" class="text-xs font-medium text-ink-400 mb-1 block">语言</label>
						<select id="book-edit-language-input" bind:value={editForm.language} class="w-full rounded-lg border border-ink-700/50 bg-ink-800/50 px-3 py-2 text-sm text-ink-100 outline-none focus:border-accent-500/30">
							<option value="zh">中文</option>
							<option value="en">English</option>
							<option value="ja">日本語</option>
							<option value="ko">한국어</option>
						</select>
					</div>
				</div>
				<div>
					<label for="book-edit-genres-input" class="text-xs font-medium text-ink-400 mb-1 block">分类（逗号分隔）</label>
					<input id="book-edit-genres-input" bind:value={editForm.genres} placeholder="玄幻, 仙侠, 武侠" class="w-full rounded-lg border border-ink-700/50 bg-ink-800/50 px-3 py-2 text-sm text-ink-100 outline-none focus:border-accent-500/30" />
				</div>
				<div>
					<label for="book-edit-tags-input" class="text-xs font-medium text-ink-400 mb-1 block">标签（逗号分隔）</label>
					<input id="book-edit-tags-input" bind:value={editForm.tags} placeholder="系统流, 无敌文" class="w-full rounded-lg border border-ink-700/50 bg-ink-800/50 px-3 py-2 text-sm text-ink-100 outline-none focus:border-accent-500/30" />
				</div>
			</div>
			<div class="flex justify-end gap-3 mt-6">
				<button type="button" onclick={() => showEditModal = false} class="px-4 py-2 text-sm text-ink-400 hover:text-ink-200 transition-colors">取消</button>
				<button type="button" onclick={saveEdit} class="px-5 py-2 rounded-lg bg-accent-500 text-ink-950 text-sm font-medium hover:bg-accent-400 transition-colors">保存</button>
			</div>
		</div>
	</div>
{/if}

{#if book}
	<AddToCollectionDialog bind:open={collectionDialogOpen} bookIds={[book.id]} />
{/if}
