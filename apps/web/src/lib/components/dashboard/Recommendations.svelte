<script lang="ts">
	import { api } from '$services/api';
	import { Sparkles } from 'lucide-svelte';
	import { onMount } from 'svelte';

	let books = $state.raw<Array<{ id: string; title: string; author?: string | null; cover_path?: string | null; reason?: string }>>([]);
	let loading = $state(true);

	onMount(async () => {
		try {
			const data = await api.getRecommendations();
			// data is an array of groups; flatten all books from all groups
			const allBooks: typeof books = [];
			for (const group of data) {
				if (group.books && Array.isArray(group.books)) {
					for (const book of group.books) {
						allBooks.push({ ...book, reason: group.category });
					}
				}
			}
			books = allBooks.slice(0, 8);
		} catch {
			books = [];
		} finally {
			loading = false;
		}
	});

	function normalizeCoverPath(path: string | null | undefined): string | null {
		if (!path) return null;
		if (path.startsWith('/api/') || path.startsWith('http') || path.startsWith('data:')) return path;
		return `/api/covers/${path}`;
	}
</script>

<div class="rounded-xl border border-ink-800/50 bg-ink-900/80 p-5">
	<div class="mb-4 flex items-center justify-between gap-3">
		<div class="flex items-center gap-2">
			<Sparkles class="h-4 w-4 text-amber-400" />
			<h3 class="text-sm font-medium text-ink-200">探索预览</h3>
		</div>
		<a href="/discover" class="rounded px-2 py-1 text-xs text-ink-400 transition-colors hover:text-accent-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70">查看全部</a>
	</div>

	{#if loading}
		<div class="space-y-3">
			{#each Array(3) as _}
				<div class="h-12 rounded-lg bg-ink-800/30 animate-pulse"></div>
			{/each}
		</div>
	{:else if books.length === 0}
		<p class="text-xs text-ink-500">暂无探索内容，多阅读几本书后再来看看</p>
	{:else}
		<div class="space-y-2">
			{#each books as book}
				{@const cover = normalizeCoverPath(book.cover_path)}
				<a
					href="/library/{book.id}"
					class="group flex items-center gap-3 rounded-lg p-2 transition-colors hover:bg-ink-800/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
				>
					{#if cover}
						<img src={cover} alt="" width="32" height="44" class="h-11 w-8 flex-shrink-0 rounded object-cover" loading="lazy" />
					{:else}
						<div class="h-11 w-8 flex-shrink-0 rounded bg-ink-800/60"></div>
					{/if}
					<div class="min-w-0 flex-1">
						<p class="text-sm text-ink-200 truncate group-hover:text-accent-300 transition-colors">{book.title}</p>
						{#if book.author}
							<p class="text-xs text-ink-500 truncate">{book.author}</p>
						{/if}
					</div>
				</a>
			{/each}
		</div>
	{/if}
</div>
