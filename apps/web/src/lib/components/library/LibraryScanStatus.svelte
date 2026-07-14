<script lang="ts">
	import { api } from '$services/api';
	import { HardDrive, Folder, FileText, Clock, AlertCircle } from 'lucide-svelte';

	let {
		libraryId,
	} = $props<{
		libraryId: string;
	}>();

	interface ScanStatus {
		status: 'idle' | 'scanning' | 'processing' | 'complete' | 'error';
		total_files: number;
		processed_files: number;
		new_books: number;
		errors: string[];
		started_at?: string;
		elapsed_seconds?: number;
	}

	let scanStatus = $state<ScanStatus>({
		status: 'idle',
		total_files: 0,
		processed_files: 0,
		new_books: 0,
		errors: [],
	});

	let polling = $state(false);

	$effect(() => {
		if (libraryId) {
			pollStatus();
		}
		return () => { polling = false; };
	});

	async function pollStatus() {
		polling = true;
		while (polling) {
			try {
				scanStatus = await api.getLibraryScanStatus(libraryId);
				if (scanStatus.status === 'complete' || scanStatus.status === 'idle' || scanStatus.status === 'error') {
					polling = false;
				}
			} catch { /* ignore */ }
			if (polling) {
				await new Promise(r => setTimeout(r, 2000));
			}
		}
	}

	async function startScan() {
		try {
			await api.scanLibrary(libraryId);
			scanStatus = { ...scanStatus, status: 'scanning', processed_files: 0, errors: [] };
			polling = true;
			pollStatus();
		} catch { /* ignore */ }
	}

	let progressPercent = $derived(
		scanStatus.total_files > 0
			? Math.round((scanStatus.processed_files / scanStatus.total_files) * 100)
			: 0
	);

	const statusLabels: Record<string, string> = {
		idle: '空闲',
		scanning: '扫描文件中',
		processing: '处理中',
		complete: '完成',
		error: '出错',
	};
</script>

<div class="rounded-xl border border-ink-700 bg-ink-800/50 p-5">
	<div class="flex items-center justify-between mb-4">
		<div class="flex items-center gap-2">
			<HardDrive size={18} class="text-ink-400" />
			<h3 class="font-medium text-ink-200">书库扫描</h3>
		</div>
		<button
			class="flex items-center gap-1.5 rounded-lg bg-accent-600 hover:bg-accent-500 px-3 py-1.5 text-xs font-medium text-white transition-colors disabled:opacity-50"
			onclick={startScan}
			disabled={scanStatus.status === 'scanning' || scanStatus.status === 'processing'}
		>
			{scanStatus.status === 'scanning' || scanStatus.status === 'processing' ? '扫描中...' : '开始扫描'}
		</button>
	</div>

	<!-- Progress bar -->
	{#if scanStatus.status === 'scanning' || scanStatus.status === 'processing'}
		<div class="space-y-2 mb-4">
			<div class="flex items-center justify-between text-xs text-ink-400">
				<span>{statusLabels[scanStatus.status]}</span>
				<span>{scanStatus.processed_files} / {scanStatus.total_files}</span>
			</div>
			<div class="h-2 rounded-full bg-ink-700 overflow-hidden">
				<div
					class="h-full bg-gradient-to-r from-accent-600 to-accent-400 rounded-full transition-all duration-500"
					style="width: {progressPercent}%"
				></div>
			</div>
		</div>
	{/if}

	<!-- Stats -->
	<div class="grid grid-cols-3 gap-3 text-center">
		<div class="rounded-lg bg-ink-800 p-2">
			<div class="flex items-center justify-center gap-1 text-ink-400 mb-1">
				<Folder size={12} />
			</div>
			<p class="text-lg font-semibold text-ink-100">{scanStatus.total_files}</p>
			<p class="text-xs text-ink-500">发现文件</p>
		</div>
		<div class="rounded-lg bg-ink-800 p-2">
			<div class="flex items-center justify-center gap-1 text-ink-400 mb-1">
				<FileText size={12} />
			</div>
			<p class="text-lg font-semibold text-ink-100">{scanStatus.new_books}</p>
			<p class="text-xs text-ink-500">新增书籍</p>
		</div>
		<div class="rounded-lg bg-ink-800 p-2">
			<div class="flex items-center justify-center gap-1 text-ink-400 mb-1">
				<Clock size={12} />
			</div>
			<p class="text-lg font-semibold text-ink-100">{scanStatus.elapsed_seconds ?? 0}s</p>
			<p class="text-xs text-ink-500">耗时</p>
		</div>
	</div>

	<!-- Errors -->
	{#if scanStatus.errors.length > 0}
		<div class="mt-4 rounded-lg border border-red-900/50 bg-red-950/20 p-3">
			<div class="flex items-center gap-1.5 text-red-400 text-xs font-medium mb-2">
				<AlertCircle size={12} />
				{scanStatus.errors.length} 个错误
			</div>
			<div class="max-h-24 overflow-y-auto space-y-1">
				{#each scanStatus.errors.slice(0, 5) as error}
					<p class="text-xs text-red-300/70 truncate">{error}</p>
				{/each}
			</div>
		</div>
	{/if}
</div>
