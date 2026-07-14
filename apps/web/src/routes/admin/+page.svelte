<script lang="ts">
	import { api } from '$services/api';
	import { onMount, onDestroy } from 'svelte';
	import { toast } from 'svelte-sonner';
	import { Database, CircleDot, Hexagon, Search, FolderSearch, Cpu, Users, Clock, ScrollText, ListChecks } from 'lucide-svelte';

	let loading = $state(true);

	let health = $state({
		status: 'unknown',
		database: false,
		redis: false,
		qdrant: false,
		meilisearch: false,
		version: '0.1.0',
		uptime_seconds: 0,
	});

	let systemStats = $state({
		total_books: 0,
		total_annotations: 0,
		total_entities: 0,
		total_chapters: 0,
		storage_used_bytes: 0,
		tasks_pending: 0,
		tasks_completed: 0,
	});

	let pollInterval: ReturnType<typeof setInterval>;
	let initialLoadFailed = $state(false);

	onMount(async () => {
		await loadDashboard();
		// Only poll if initial load succeeded
		if (!initialLoadFailed) {
			pollInterval = setInterval(loadDashboard, 30000);
		}
	});

	onDestroy(() => {
		if (pollInterval) clearInterval(pollInterval);
	});

	async function loadDashboard() {
		try {
			const h = await api.getHealth();
			health = h;
		} catch {
			// Health check failed silently
		}

		try {
			const s = await api.getSystemStats();
			systemStats = s;
		} catch (err: unknown) {
			if (err instanceof Error && 'status' in err && (err as { status: number }).status === 404) {
				// Endpoint not available yet — don't spam toasts
				initialLoadFailed = true;
			} else {
				const msg = err instanceof Error ? err.message : '服务不可达';
				toast.error('管理面板加载失败', { description: msg });
			}
		} finally {
			loading = false;
		}
	}

	function formatUptime(seconds: number): string {
		const d = Math.floor(seconds / 86400);
		const h = Math.floor((seconds % 86400) / 3600);
		const m = Math.floor((seconds % 3600) / 60);
		if (d > 0) return `${d}天 ${h}小时`;
		if (h > 0) return `${h}小时 ${m}分钟`;
		return `${m}分钟`;
	}

	function formatBytes(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
		return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
	}
</script>

<svelte:head>
	<title>Nova Reader — 系统仪表盘</title>
</svelte:head>

<div class="mx-auto max-w-[1600px] px-4 py-6 sm:px-6 lg:px-8 space-y-6 animate-fade-in">
	<!-- Header -->
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-bold text-ink-50">系统仪表盘</h1>
			<p class="mt-1 text-sm text-ink-400">服务健康状态与资源监控</p>
		</div>
		<a
			href="/libraries"
			class="flex items-center gap-2 rounded-lg bg-accent-500 px-4 py-2 text-sm font-medium text-ink-950 hover:bg-accent-400 transition-colors"
		>
			<FolderSearch size={16} strokeWidth={2} />
			书库扫描
		</a>
	</div>

	{#if loading}
		<div class="grid grid-cols-2 gap-4 md:grid-cols-4">
			{#each Array(4) as _}
				<div class="h-28 rounded-xl bg-ink-900/30 animate-pulse"></div>
			{/each}
		</div>
	{:else}
		<!-- Health Status -->
		<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-5">
			<div class="flex items-center justify-between mb-4">
				<h2 class="text-sm font-medium text-ink-300">服务健康</h2>
				<span class="flex items-center gap-1.5 text-xs" class:text-emerald-400={health.status === 'ok'} class:text-red-400={health.status !== 'ok'}>
					<span class="h-2 w-2 rounded-full" class:bg-emerald-400={health.status === 'ok'} class:bg-red-400={health.status !== 'ok'}></span>
					{health.status === 'ok' ? '全部正常' : '存在异常'}
				</span>
			</div>
			<div class="grid grid-cols-2 gap-3 md:grid-cols-4">
				{#each [
					{ name: 'PostgreSQL', ok: health.database, icon: 'db' },
					{ name: 'Redis', ok: health.redis, icon: 'redis' },
					{ name: 'Qdrant', ok: health.qdrant, icon: 'qdrant' },
					{ name: 'Meilisearch', ok: health.meilisearch, icon: 'search' },
				] as service}
					<div class="flex items-center gap-3 rounded-lg border border-ink-800/30 bg-ink-900/50 px-3 py-2.5">
						<div class="flex h-8 w-8 items-center justify-center rounded-lg bg-ink-800/80">
							{#if service.icon === 'db'}
								<Database size={14} strokeWidth={1.8} class="text-ink-400" />
							{:else if service.icon === 'redis'}
								<CircleDot size={14} strokeWidth={1.8} class="text-ink-400" />
							{:else if service.icon === 'qdrant'}
								<Hexagon size={14} strokeWidth={1.8} class="text-ink-400" />
							{:else}
								<Search size={14} strokeWidth={1.8} class="text-ink-400" />
							{/if}
						</div>
						<div class="flex-1">
							<div class="text-xs font-medium text-ink-200">{service.name}</div>
							<div class="text-[10px]" class:text-emerald-400={service.ok} class:text-red-400={!service.ok}>
								{service.ok ? '运行中' : '离线'}
							</div>
						</div>
						<div class="h-2.5 w-2.5 rounded-full" class:bg-emerald-400={service.ok} class:bg-red-400={!service.ok}></div>
					</div>
				{/each}
			</div>
		</div>

		<!-- System Stats -->
		<div class="grid grid-cols-2 gap-3 md:grid-cols-4">
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<div class="text-xs text-ink-500">书籍总数</div>
				<div class="mt-1 text-2xl font-bold text-ink-100">{(systemStats.total_books ?? 0).toLocaleString()}</div>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<div class="text-xs text-ink-500">章节总数</div>
				<div class="mt-1 text-2xl font-bold text-ink-100">{(systemStats.total_chapters ?? 0).toLocaleString()}</div>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<div class="text-xs text-ink-500">实体/批注</div>
				<div class="mt-1 text-2xl font-bold text-ink-100">{(systemStats.total_entities ?? 0).toLocaleString()} / {(systemStats.total_annotations ?? 0).toLocaleString()}</div>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<div class="text-xs text-ink-500">存储空间</div>
				<div class="mt-1 text-2xl font-bold text-ink-100">{formatBytes(systemStats.storage_used_bytes ?? 0)}</div>
			</div>
		</div>

		<!-- Server Info -->
		<div class="grid gap-4 md:grid-cols-2">
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-5">
				<h3 class="text-sm font-medium text-ink-300 mb-3">服务器信息</h3>
				<div class="space-y-2 text-sm">
					<div class="flex justify-between">
						<span class="text-ink-400">版本</span>
						<span class="text-ink-200 font-mono">{health.version}</span>
					</div>
					<div class="flex justify-between">
						<span class="text-ink-400">运行时间</span>
						<span class="text-ink-200">{formatUptime(health.uptime_seconds)}</span>
					</div>
					<div class="flex justify-between">
						<span class="text-ink-400">待处理任务</span>
						<span class="text-ink-200">{systemStats.tasks_pending}</span>
					</div>
					<div class="flex justify-between">
						<span class="text-ink-400">已完成任务</span>
						<span class="text-ink-200">{systemStats.tasks_completed}</span>
					</div>
				</div>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-5">
				<h3 class="text-sm font-medium text-ink-300 mb-3">快速操作</h3>
				<div class="space-y-2">
					<a href="/libraries" class="w-full flex items-center gap-3 rounded-lg border border-ink-700/50 px-4 py-2.5 text-left text-sm text-ink-200 hover:bg-ink-800/30 transition-colors">
						<FolderSearch size={15} strokeWidth={1.8} class="text-ink-400" /> 书库扫描与管理
					</a>
					<a href="/admin/ai-usage" class="flex items-center gap-3 w-full rounded-lg border border-ink-700/50 px-4 py-2.5 text-left text-sm text-ink-200 hover:bg-ink-800/30 transition-colors">
						<Cpu size={15} strokeWidth={1.8} class="text-ink-400" /> AI 用量统计
					</a>
					<a href="/admin/users" class="flex items-center gap-3 w-full rounded-lg border border-ink-700/50 px-4 py-2.5 text-left text-sm text-ink-200 hover:bg-ink-800/30 transition-colors">
						<Users size={15} strokeWidth={1.8} class="text-ink-400" /> 用户管理
					</a>
					<a href="/admin/jobs" class="flex items-center gap-3 w-full rounded-lg border border-ink-700/50 px-4 py-2.5 text-left text-sm text-ink-200 hover:bg-ink-800/30 transition-colors">
						<Clock size={15} strokeWidth={1.8} class="text-ink-400" /> 定时任务
					</a>
					<a href="/admin/logs" class="flex items-center gap-3 w-full rounded-lg border border-ink-700/50 px-4 py-2.5 text-left text-sm text-ink-200 hover:bg-ink-800/30 transition-colors">
						<ScrollText size={15} strokeWidth={1.8} class="text-ink-400" /> 系统日志
					</a>
					<a href="/tasks" class="flex items-center gap-3 w-full rounded-lg border border-ink-700/50 px-4 py-2.5 text-left text-sm text-ink-200 hover:bg-ink-800/30 transition-colors">
						<ListChecks size={15} strokeWidth={1.8} class="text-ink-400" /> 查看任务队列
					</a>
					<a href="/admin/ai-settings" class="block w-full rounded-lg border border-ink-700/50 px-4 py-2.5 text-left text-sm text-ink-200 hover:bg-ink-800/30 transition-colors">
						⚙️ AI 配置
					</a>
					<a href="/library/duplicates" class="block w-full rounded-lg border border-ink-700/50 px-4 py-2.5 text-left text-sm text-ink-200 hover:bg-ink-800/30 transition-colors">
						🔍 重复检测
					</a>
				</div>
			</div>
		</div>
	{/if}
</div>
