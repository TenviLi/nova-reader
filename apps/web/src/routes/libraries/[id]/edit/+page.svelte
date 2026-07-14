<script lang="ts">
import { getErrorMessage } from '$lib/utils';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { api } from '$services/api';
	import { onMount } from 'svelte';
	import { toast } from 'svelte-sonner';
	import { Library, Save, Trash2, ArrowLeft, RefreshCw, FolderOpen } from 'lucide-svelte';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import type { Library as LibraryType } from '$types/models';

	const libraryId = $page.params.id!;

	let library = $state<LibraryType | null>(null);
	let loading = $state(true);
	let saving = $state(false);

	// Form state
	let name = $state('');
	let description = $state('');
	let autoScan = $state(false);
	let scanIntervalSecs = $state(3600);
	let includeExtensions = $state('');
	let excludePatterns = $state('');

	onMount(async () => {
		try {
			library = await api.getLibrary(libraryId);
			if (library) {
				name = library.name;
				description = library.description ?? '';
				autoScan = library.auto_scan;
				scanIntervalSecs = library.scan_interval_secs;
				includeExtensions = (library.include_extensions ?? []).join(', ');
				excludePatterns = (library.exclude_patterns ?? []).join('\n');
			}
		} catch (e: unknown) {
			toast.error('加载书库信息失败');
		} finally {
			loading = false;
		}
	});

	async function handleSave() {
		saving = true;
		try {
			await api.updateLibrary(libraryId, {
				name,
				description: description || null,
				auto_scan: autoScan,
				scan_interval_secs: scanIntervalSecs,
				include_extensions: includeExtensions.split(',').map(s => s.trim()).filter(Boolean),
				exclude_patterns: excludePatterns.split('\n').map(s => s.trim()).filter(Boolean),
			});
			toast.success('书库设置已保存');
		} catch (e: unknown) {
			toast.error(`保存失败: ${getErrorMessage(e)}`);
		} finally {
			saving = false;
		}
	}

	async function handleDelete() {
		if (!confirm(`确定删除书库「${name}」？此操作不会删除实际文件，但会移除所有书籍记录。`)) return;
		try {
			await api.deleteLibrary(libraryId);
			toast.success('书库已删除');
			goto('/libraries');
		} catch (e: unknown) {
			toast.error(`删除失败: ${getErrorMessage(e)}`);
		}
	}

	async function handleScan() {
		try {
			await api.scanLibrary(libraryId);
			toast.success('扫描任务已触发');
		} catch {
			toast.error('扫描失败');
		}
	}
</script>

<svelte:head>
	<title>书库设置 — {library?.name ?? 'Nova Reader'}</title>
</svelte:head>

<div class="mx-auto max-w-3xl px-4 py-6 sm:px-6 lg:px-8 space-y-6 animate-fade-in">
	{#if loading}
		<div class="space-y-4">
			<div class="h-8 w-48 rounded-lg bg-ink-900/50 animate-pulse"></div>
			<div class="h-64 rounded-xl bg-ink-900/50 animate-pulse"></div>
		</div>
	{:else if library}
		<!-- Header -->
		<div class="flex items-center gap-3">
			<a
				href="/libraries/{libraryId}"
				class="inline-flex items-center gap-1.5 text-sm text-ink-400 hover:text-ink-200 transition-colors"
			>
				<ArrowLeft size={16} />
				返回书库
			</a>
		</div>

		<div class="flex items-center gap-3">
			<div class="flex h-10 w-10 items-center justify-center rounded-xl bg-accent-500/10 ring-1 ring-accent-500/20">
				<Library size={20} class="text-accent-400" />
			</div>
			<div>
				<h1 class="text-2xl font-bold text-ink-50">书库设置</h1>
				<p class="text-sm text-ink-400">{library.root_path}</p>
			</div>
		</div>

		<!-- Settings Form -->
		<div class="space-y-6">
			<!-- Basic Info -->
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/50 p-6 space-y-4">
				<h2 class="text-base font-semibold text-ink-100">基本信息</h2>

				<div class="space-y-2">
					<label for="library-name" class="block text-sm text-ink-300">名称</label>
					<Input id="library-name" bind:value={name} class="bg-ink-800/50 border-ink-700/50" />
				</div>

				<div class="space-y-2">
					<label for="library-description" class="block text-sm text-ink-300">描述</label>
					<textarea
						id="library-description"
						bind:value={description}
						rows="3"
						class="w-full rounded-lg border border-ink-700/50 bg-ink-800/50 px-3 py-2 text-sm text-ink-100 placeholder:text-ink-500 focus:border-accent-500/50 focus:outline-none resize-none"
						placeholder="为这个书库添加描述..."
					></textarea>
				</div>

				<div class="space-y-2">
					<p id="library-path-label" class="block text-sm text-ink-300">路径</p>
					<div aria-labelledby="library-path-label" class="flex items-center gap-2 rounded-lg border border-ink-700/50 bg-ink-800/30 px-3 py-2 text-sm text-ink-400">
						<FolderOpen size={14} />
						<span class="font-mono">{library.root_path}</span>
					</div>
					<p class="text-xs text-ink-500">路径创建后不可更改</p>
				</div>
			</div>

			<!-- Scan Settings -->
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/50 p-6 space-y-4">
				<h2 class="text-base font-semibold text-ink-100">扫描设置</h2>

				<div class="flex items-center justify-between">
					<div>
						<p class="text-sm text-ink-200">自动扫描</p>
						<p class="text-xs text-ink-500">定期自动检测新文件</p>
					</div>
					<button
						type="button"
						onclick={() => autoScan = !autoScan}
						aria-label={autoScan ? '关闭自动扫描' : '开启自动扫描'}
						aria-pressed={autoScan}
						class="relative h-6 w-11 rounded-full transition-colors {autoScan ? 'bg-accent-500' : 'bg-ink-700'}"
					>
						<span class="absolute top-0.5 left-0.5 h-5 w-5 rounded-full bg-white transition-transform {autoScan ? 'translate-x-5' : ''}"></span>
					</button>
				</div>

				{#if autoScan}
					<div class="space-y-2">
						<label for="library-scan-interval" class="block text-sm text-ink-300">扫描间隔</label>
						<select
							id="library-scan-interval"
							bind:value={scanIntervalSecs}
							class="w-full rounded-lg border border-ink-700/50 bg-ink-800/50 px-3 py-2 text-sm text-ink-100"
						>
							<option value={1800}>30 分钟</option>
							<option value={3600}>1 小时</option>
							<option value={7200}>2 小时</option>
							<option value={21600}>6 小时</option>
							<option value={43200}>12 小时</option>
							<option value={86400}>24 小时</option>
						</select>
					</div>
				{/if}

				<div class="space-y-2">
					<label for="library-include-extensions" class="block text-sm text-ink-300">文件扩展名 (逗号分隔)</label>
					<Input
						id="library-include-extensions"
						bind:value={includeExtensions}
						placeholder=".epub, .txt, .mobi, .azw3, .pdf"
						class="bg-ink-800/50 border-ink-700/50"
					/>
					<p class="text-xs text-ink-500">留空则扫描所有支持的格式</p>
				</div>

				<div class="space-y-2">
					<label for="library-exclude-patterns" class="block text-sm text-ink-300">排除规则 (每行一个)</label>
					<textarea
						id="library-exclude-patterns"
						bind:value={excludePatterns}
						rows="4"
						class="w-full rounded-lg border border-ink-700/50 bg-ink-800/50 px-3 py-2 text-sm text-ink-100 placeholder:text-ink-500 focus:border-accent-500/50 focus:outline-none resize-none font-mono"
						placeholder={"**/node_modules/**\n**/.git/**\n**/temp/**"}
					></textarea>
				</div>

				<!-- Scan Status -->
				<div class="rounded-lg border border-ink-800/50 bg-ink-900/30 p-4">
					<div class="flex items-center justify-between">
						<div>
							<p class="text-sm text-ink-200">扫描状态</p>
							<p class="text-xs text-ink-500 mt-0.5">
								{#if library.last_scan_at}
									上次扫描: {new Date(library.last_scan_at).toLocaleString('zh-CN')}
									{#if library.last_scan_duration_ms}
										(耗时 {(library.last_scan_duration_ms / 1000).toFixed(1)}s)
									{/if}
								{:else}
									从未扫描
								{/if}
							</p>
						</div>
						<button
							type="button"
							onclick={handleScan}
							class="inline-flex items-center gap-1.5 rounded-lg border border-ink-700/50 px-3 py-1.5 text-sm text-ink-300 hover:bg-ink-800/50 transition-colors"
						>
							<RefreshCw size={14} />
							立即扫描
						</button>
					</div>
				</div>
			</div>

			<!-- Statistics -->
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/50 p-6 space-y-4">
				<h2 class="text-base font-semibold text-ink-100">统计信息</h2>
				<div class="grid grid-cols-3 gap-4">
					<div class="text-center">
						<p class="text-2xl font-bold text-ink-100">{library.book_count}</p>
						<p class="text-xs text-ink-400 mt-1">书籍</p>
					</div>
					<div class="text-center">
						<p class="text-2xl font-bold text-ink-100">{library.series_count}</p>
						<p class="text-xs text-ink-400 mt-1">系列</p>
					</div>
					<div class="text-center">
						<p class="text-2xl font-bold text-ink-100">
							{library.total_size_bytes ? (library.total_size_bytes / 1024 / 1024).toFixed(1) + ' MB' : '—'}
						</p>
						<p class="text-xs text-ink-400 mt-1">总大小</p>
					</div>
				</div>
			</div>

			<!-- Actions -->
			<div class="flex items-center justify-between">
				<button
					type="button"
					onclick={handleDelete}
					class="inline-flex items-center gap-2 rounded-lg border border-error/30 bg-error/5 px-4 py-2 text-sm text-error hover:bg-error/10 transition-colors"
				>
					<Trash2 size={14} />
					删除书库
				</button>
				<button
					type="button"
					onclick={handleSave}
					disabled={saving}
					class="inline-flex items-center gap-2 rounded-lg bg-accent-500 px-5 py-2 text-sm font-medium text-ink-950 hover:bg-accent-400 disabled:opacity-50 transition-colors"
				>
					<Save size={14} />
					{saving ? '保存中...' : '保存设置'}
				</button>
			</div>
		</div>
	{:else}
		<div class="text-center py-20">
			<Library size={48} class="mx-auto text-ink-600 mb-3" />
			<p class="text-ink-300">书库不存在或已被删除</p>
			<a href="/libraries" class="mt-3 inline-block text-sm text-accent-400 hover:text-accent-300">返回书库管理</a>
		</div>
	{/if}
</div>
