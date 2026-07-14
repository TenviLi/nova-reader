<script lang="ts">
	import { readerStore } from '$stores/reader.svelte';
	import { RefreshCw, Monitor, Smartphone } from 'lucide-svelte';
	import { fly } from 'svelte/transition';

	let conflict = $derived(readerStore.syncConflict);
</script>

{#if conflict}
	<div
		transition:fly={{ y: -20, duration: 200 }}
		class="fixed top-4 left-1/2 -translate-x-1/2 z-50 w-[90vw] max-w-md rounded-xl border border-amber-500/30 bg-ink-950 shadow-2xl shadow-amber-900/20 p-5"
	>
		<div class="flex items-start gap-3 mb-4">
			<div class="rounded-lg bg-amber-500/10 p-2">
				<RefreshCw class="w-5 h-5 text-amber-400" />
			</div>
			<div>
				<h3 class="text-sm font-semibold text-ink-100">阅读进度冲突</h3>
				<p class="text-xs text-ink-400 mt-0.5">检测到另一设备有更新的阅读进度</p>
			</div>
		</div>

		<div class="grid grid-cols-2 gap-3 mb-4">
			<!-- Server (other device) -->
			<div class="rounded-lg border border-ink-700 bg-ink-900/50 p-3">
				<div class="flex items-center gap-1.5 mb-2">
					<Smartphone class="w-3.5 h-3.5 text-accent-400" />
					<span class="text-xs font-medium text-ink-300">其他设备</span>
				</div>
				<div class="text-xs text-ink-400 space-y-1">
					<div>章节: 第 {(conflict.server_progress.chapter_index ?? conflict.server_progress.current_chapter ?? 0) + 1} 章</div>
					<div>进度: {Math.round((conflict.server_progress.progress ?? 0) * 100)}%</div>
				</div>
			</div>

			<!-- Client (this device) -->
			<div class="rounded-lg border border-ink-700 bg-ink-900/50 p-3">
				<div class="flex items-center gap-1.5 mb-2">
					<Monitor class="w-3.5 h-3.5 text-ink-400" />
					<span class="text-xs font-medium text-ink-300">本设备</span>
				</div>
				<div class="text-xs text-ink-400 space-y-1">
					<div>章节: 第 {(conflict.client_progress.chapter_index ?? 0) + 1} 章</div>
					<div>进度: {Math.round((conflict.client_progress.progress ?? 0) * 100)}%</div>
				</div>
			</div>
		</div>

		<div class="flex gap-2">
			<button
				onclick={() => readerStore.resolveConflict('server')}
				class="flex-1 px-3 py-2 bg-accent-600 hover:bg-accent-500 text-white text-xs font-medium rounded-lg transition-colors"
			>
				使用其他设备进度
			</button>
			<button
				onclick={() => readerStore.resolveConflict('client')}
				class="flex-1 px-3 py-2 bg-ink-800 hover:bg-ink-700 text-ink-200 text-xs font-medium rounded-lg transition-colors"
			>
				保留本设备进度
			</button>
		</div>
	</div>
{/if}
