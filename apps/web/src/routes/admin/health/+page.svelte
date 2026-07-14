<script lang="ts">
import { getErrorMessage } from '$lib/utils';
	import { api } from '$lib/services/api';
	import { toast } from 'svelte-sonner';
	import { HeartPulse, Image, BookOpen, AlertTriangle, FileWarning, RefreshCw, Trash2, Calculator } from 'lucide-svelte';

	let healthData = $state<{
		total_issues: number;
		issues: { missing_cover: number; no_chapters: number; abnormal_progress: number; zero_word_count: number };
		status: string;
	} | null>(null);

	let orphanData = $state<{
		total_checked: number;
		orphans_found: number;
		orphans: Array<{ id: string; title: string; file_path: string; library_id: string | null }>;
	} | null>(null);

	let loadingHealth = $state(false);
	let loadingOrphans = $state(false);
	let loadingRecalculate = $state(false);

	async function runHealthCheck() {
		loadingHealth = true;
		try {
			healthData = await api.getBooksHealthCheck();
			if (healthData.total_issues === 0) {
				toast.success('所有书籍数据健康');
			} else {
				toast.warning(`发现 ${healthData.total_issues} 个问题`);
			}
		} catch (e: unknown) {
			toast.error(`健康检查失败: ${getErrorMessage(e)}`);
		} finally {
			loadingHealth = false;
		}
	}

	async function runOrphanDetection() {
		loadingOrphans = true;
		try {
			orphanData = await api.detectOrphanBooks();
			if (orphanData.orphans_found === 0) {
				toast.success('未发现孤立书籍');
			} else {
				toast.warning(`发现 ${orphanData.orphans_found} 本孤立书籍`);
			}
		} catch (e: unknown) {
			toast.error(`孤立检测失败: ${getErrorMessage(e)}`);
		} finally {
			loadingOrphans = false;
		}
	}

	async function runRecalculate() {
		loadingRecalculate = true;
		try {
			const result = await api.recalculateMetadata();
			toast.success(result.message);
		} catch (e: unknown) {
			toast.error(`重算失败: ${getErrorMessage(e)}`);
		} finally {
			loadingRecalculate = false;
		}
	}
</script>

<div class="space-y-6">
	<div>
		<h1 class="text-xl font-bold text-ink-100">数据健康</h1>
		<p class="text-sm text-ink-500 mt-1">检测书籍数据完整性、孤立文件和异常进度</p>
	</div>

	<!-- Actions -->
	<div class="flex flex-wrap gap-3">
		<button
			onclick={runHealthCheck}
			disabled={loadingHealth}
			class="flex items-center gap-2 px-4 py-2 bg-accent-600 hover:bg-accent-500 disabled:opacity-50 text-white rounded-lg text-sm font-medium transition-colors"
		>
			<HeartPulse class="w-4 h-4" />
			{loadingHealth ? '检查中...' : '运行健康检查'}
		</button>
		<button
			onclick={runOrphanDetection}
			disabled={loadingOrphans}
			class="flex items-center gap-2 px-4 py-2 bg-ink-800 hover:bg-ink-700 disabled:opacity-50 text-ink-200 rounded-lg text-sm font-medium transition-colors"
		>
			<FileWarning class="w-4 h-4" />
			{loadingOrphans ? '检测中...' : '检测孤立书籍'}
		</button>
		<button
			onclick={runRecalculate}
			disabled={loadingRecalculate}
			class="flex items-center gap-2 px-4 py-2 bg-ink-800 hover:bg-ink-700 disabled:opacity-50 text-ink-200 rounded-lg text-sm font-medium transition-colors"
		>
			<Calculator class="w-4 h-4" />
			{loadingRecalculate ? '重算中...' : '重算字数与作者'}
		</button>
	</div>

	<!-- Health Check Results -->
	{#if healthData}
		<div class="rounded-xl border border-ink-800 bg-ink-950 p-5 space-y-4">
			<div class="flex items-center justify-between">
				<h2 class="text-base font-semibold text-ink-200">健康检查结果</h2>
				<span class="px-2.5 py-0.5 rounded-full text-xs font-medium {healthData.status === 'healthy' ? 'bg-green-500/20 text-green-400' : 'bg-amber-500/20 text-amber-400'}">
					{healthData.status === 'healthy' ? '全部健康' : `${healthData.total_issues} 个问题`}
				</span>
			</div>

			<div class="grid grid-cols-2 md:grid-cols-4 gap-4">
				<div class="rounded-lg bg-ink-900/50 p-3.5">
					<div class="flex items-center gap-2 mb-1.5">
						<Image class="w-4 h-4 text-ink-400" />
						<span class="text-xs text-ink-400">缺少封面</span>
					</div>
					<div class="text-lg font-bold {healthData.issues.missing_cover > 0 ? 'text-amber-400' : 'text-ink-200'}">
						{healthData.issues.missing_cover}
					</div>
				</div>
				<div class="rounded-lg bg-ink-900/50 p-3.5">
					<div class="flex items-center gap-2 mb-1.5">
						<BookOpen class="w-4 h-4 text-ink-400" />
						<span class="text-xs text-ink-400">无章节</span>
					</div>
					<div class="text-lg font-bold {healthData.issues.no_chapters > 0 ? 'text-red-400' : 'text-ink-200'}">
						{healthData.issues.no_chapters}
					</div>
				</div>
				<div class="rounded-lg bg-ink-900/50 p-3.5">
					<div class="flex items-center gap-2 mb-1.5">
						<AlertTriangle class="w-4 h-4 text-ink-400" />
						<span class="text-xs text-ink-400">进度异常</span>
					</div>
					<div class="text-lg font-bold {healthData.issues.abnormal_progress > 0 ? 'text-red-400' : 'text-ink-200'}">
						{healthData.issues.abnormal_progress}
					</div>
				</div>
				<div class="rounded-lg bg-ink-900/50 p-3.5">
					<div class="flex items-center gap-2 mb-1.5">
						<FileWarning class="w-4 h-4 text-ink-400" />
						<span class="text-xs text-ink-400">字数为零</span>
					</div>
					<div class="text-lg font-bold {healthData.issues.zero_word_count > 0 ? 'text-amber-400' : 'text-ink-200'}">
						{healthData.issues.zero_word_count}
					</div>
				</div>
			</div>
		</div>
	{/if}

	<!-- Orphan Detection Results -->
	{#if orphanData}
		<div class="rounded-xl border border-ink-800 bg-ink-950 p-5 space-y-4">
			<div class="flex items-center justify-between">
				<h2 class="text-base font-semibold text-ink-200">孤立书籍检测</h2>
				<span class="text-xs text-ink-400">
					已检查 {orphanData.total_checked} 本书
				</span>
			</div>

			{#if orphanData.orphans_found === 0}
				<p class="text-sm text-green-400">未发现孤立书籍，所有文件路径均有效。</p>
			{:else}
				<p class="text-sm text-amber-400 mb-3">
					发现 {orphanData.orphans_found} 本书的源文件已被删除，但数据库记录仍存在：
				</p>
				<div class="space-y-2 max-h-80 overflow-y-auto">
					{#each orphanData.orphans as orphan}
						<div class="flex items-center justify-between gap-3 p-3 rounded-lg bg-ink-900/50 border border-ink-800">
							<div class="min-w-0 flex-1">
								<div class="text-sm font-medium text-ink-200 truncate">{orphan.title}</div>
								<div class="text-xs text-ink-500 truncate mt-0.5">{orphan.file_path}</div>
							</div>
								<a
									href="/library/{orphan.id}"
									class="shrink-0 text-xs text-accent-400 hover:text-accent-300"
								>
								查看
							</a>
						</div>
					{/each}
				</div>
			{/if}
		</div>
	{/if}

	{#if !healthData && !orphanData}
		<div class="text-center py-12 text-ink-500">
			<HeartPulse class="w-12 h-12 mx-auto mb-4 opacity-30" />
			<p>点击上方按钮运行检查</p>
		</div>
	{/if}
</div>
