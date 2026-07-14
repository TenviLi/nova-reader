<script lang="ts">
	import { Bell, Check, XCircle, AlertTriangle, Info, BookOpen, Cpu, Users, Library as LibraryIcon, Trash2 } from 'lucide-svelte';
	import {
		useNotifications,
		useMarkNotificationRead,
		useMarkAllNotificationsRead,
		useDeleteNotification,
		useClearNotifications,
	} from '$lib/queries';

	const notifCategories = [
		{ key: 'all', label: '全部', icon: Bell },
		{ key: 'system', label: '系统', icon: Info },
		{ key: 'reading', label: '阅读', icon: BookOpen },
		{ key: 'ai', label: 'AI', icon: Cpu },
		{ key: 'library', label: '书库', icon: LibraryIcon },
		{ key: 'social', label: '社交', icon: Users },
	];

	let filter = $state<'all' | 'unread'>('all');
	let category = $state<'all' | 'system' | 'reading' | 'ai' | 'library' | 'social'>('all');

	const notifQuery = useNotifications(() => ({
		category,
		unreadOnly: filter === 'unread',
		limit: 100,
	}));
	const markReadMut = useMarkNotificationRead();
	const markAllMut = useMarkAllNotificationsRead();
	const deleteMut = useDeleteNotification();
	const clearMut = useClearNotifications();

	let notifications = $derived(notifQuery.data?.items ?? []);
	let unreadCount = $derived(notifQuery.data?.unread ?? 0);
	let loading = $derived(notifQuery.isLoading);

	function markAllRead() {
		markAllMut.mutate();
	}

	function markRead(id: string) {
		markReadMut.mutate(id);
	}

	function removeNotification(id: string) {
		deleteMut.mutate(id);
	}

	function clearRead() {
		clearMut.mutate(true);
	}

	function timeAgo(dateStr: string): string {
		const now = Date.now();
		const date = new Date(dateStr).getTime();
		const diff = now - date;
		if (diff < 60000) return '刚刚';
		if (diff < 3600000) return `${Math.floor(diff / 60000)} 分钟前`;
		if (diff < 86400000) return `${Math.floor(diff / 3600000)} 小时前`;
		return `${Math.floor(diff / 86400000)} 天前`;
	}
</script>

<svelte:head>
	<title>Nova Reader — 通知</title>
</svelte:head>

<div class="mx-auto max-w-3xl px-4 py-6 sm:px-6 lg:px-8 space-y-6 animate-fade-in">
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-bold text-ink-50">通知中心</h1>
			<p class="mt-0.5 text-sm text-ink-400">
				{unreadCount > 0 ? `${unreadCount} 条未读` : '全部已读'}
			</p>
		</div>
		<div class="flex items-center gap-3">
			<div class="flex gap-1 rounded-lg border border-ink-700/50 bg-ink-900/50 p-0.5">
				<button
					onclick={() => filter = 'all'}
					class="rounded-md px-3 py-1 text-xs transition-colors"
					class:bg-ink-700={filter === 'all'}
					class:text-ink-100={filter === 'all'}
					class:text-ink-400={filter !== 'all'}
				>全部</button>
				<button
					onclick={() => filter = 'unread'}
					class="rounded-md px-3 py-1 text-xs transition-colors"
					class:bg-ink-700={filter === 'unread'}
					class:text-ink-100={filter === 'unread'}
					class:text-ink-400={filter !== 'unread'}
				>未读</button>
			</div>
			{#if unreadCount > 0}
				<button
					onclick={markAllRead}
					class="text-xs text-accent-400 hover:text-accent-300 transition-colors"
				>全部标为已读</button>
			{/if}
			<button
				onclick={clearRead}
				class="text-xs text-ink-500 hover:text-ink-300 transition-colors"
			>清除已读</button>
		</div>
	</div>

	<!-- Category tabs -->
	<div class="flex gap-2 overflow-x-auto pb-1">
		{#each notifCategories as cat}
			<button
				onclick={() => category = cat.key as typeof category}
				class="flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium whitespace-nowrap transition-colors {category === cat.key ? 'bg-accent-500/15 text-accent-300 ring-1 ring-accent-500/30' : 'text-ink-400 hover:text-ink-200 hover:bg-ink-800/50'}"
			>
				<cat.icon size={13} />
				{cat.label}
			</button>
		{/each}
	</div>

	{#if loading}
		<div class="space-y-3">
			{#each Array(5) as _}
				<div class="h-20 rounded-xl bg-ink-900/50 animate-pulse"></div>
			{/each}
		</div>
	{:else if notifications.length === 0}
		<div class="text-center py-20">
			<div class="w-16 h-16 mx-auto mb-4 bg-ink-800/30 rounded-full flex items-center justify-center ring-1 ring-ink-700/30">
				<Bell size={28} strokeWidth={1.5} class="text-ink-500" />
			</div>
			<p class="text-ink-300">没有通知</p>
			<p class="mt-1 text-xs text-ink-500">扫描书库、运行 AI 分析等操作完成后会在此提醒你</p>
		</div>
	{:else}
		<div class="space-y-2">
			{#each notifications as notification (notification.id)}
				<div
					class="group w-full flex items-start gap-4 rounded-xl border p-4 text-left transition-all hover:border-ink-700/70 hover:bg-ink-900/50 {notification.read ? 'border-ink-800/50 bg-ink-900/30' : 'border-accent-500/20 bg-accent-500/5'}"
				>
					<!-- Icon -->
					<div
						class="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg {notification.level === 'info' ? 'bg-info/10' : ''} {notification.level === 'success' ? 'bg-success/10' : ''} {notification.level === 'warning' ? 'bg-warning/10' : ''} {notification.level === 'error' ? 'bg-error/10' : ''}"
						class:text-info={notification.level === 'info'}
						class:text-success={notification.level === 'success'}
						class:text-warning={notification.level === 'warning'}
						class:text-error={notification.level === 'error'}
					>
						{#if notification.level === 'success'}
							<Check size={16} strokeWidth={2} />
						{:else if notification.level === 'error'}
							<XCircle size={16} strokeWidth={2} />
						{:else if notification.level === 'warning'}
							<AlertTriangle size={16} strokeWidth={2} />
						{:else}
							<Info size={16} strokeWidth={2} />
						{/if}
					</div>

					<!-- Content -->
						<div class="flex-1 min-w-0 space-y-1 text-left">
							<div class="flex items-center gap-2">
								<h4 class="text-sm font-medium text-ink-100 line-clamp-2">{notification.title}</h4>
								{#if !notification.read}
									<span class="h-2 w-2 shrink-0 rounded-full bg-accent-500"></span>
								{/if}
						</div>
						{#if notification.body}
							<p class="text-sm leading-relaxed text-ink-400 whitespace-pre-wrap">{notification.body}</p>
						{/if}
						{#if notification.link}
							<a
								href={notification.link}
								onclick={(e) => e.stopPropagation()}
								class="inline-flex text-xs text-accent-400 hover:text-accent-300 transition-colors"
							>
								查看详情
							</a>
						{:else if notification.book_id}
							<a
								href="/library/{notification.book_id}"
								onclick={(e) => e.stopPropagation()}
								class="inline-flex text-xs text-accent-400 hover:text-accent-300 transition-colors"
							>
									查看相关书籍
								</a>
							{/if}
						</div>

						<!-- Time + actions -->
						<div class="flex shrink-0 flex-col items-end gap-2">
							<span class="text-xs text-ink-500">{timeAgo(notification.created_at)}</span>
							{#if !notification.read}
								<button
									onclick={() => markRead(notification.id)}
									class="text-xs text-accent-400 transition-colors hover:text-accent-300"
								>
									标为已读
								</button>
							{/if}
							<button
								onclick={() => removeNotification(notification.id)}
							class="text-ink-600 opacity-0 transition-opacity hover:text-error group-hover:opacity-100"
							aria-label="删除通知"
						>
							<Trash2 size={14} />
						</button>
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>
