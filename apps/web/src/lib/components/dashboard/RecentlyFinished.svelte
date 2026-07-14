<script lang="ts">
	import { api } from '$services/api';
	import { onMount } from 'svelte';
	import { CheckCircle, ArrowRight } from 'lucide-svelte';

	import type { Book } from '$types/models';

	let books = $state<Book[]>([]);
	let loading = $state(true);

	onMount(async () => {
		try {
			const res = await api.getBooks({ reading_status: 'completed', sort_by: 'updated_at', per_page: 10 });
			books = res.data ?? [];
		} catch {
			// graceful fallback
		} finally {
			loading = false;
		}
	});
</script>

<div class="rounded-xl border border-ink-800/40 bg-ink-900/30 p-5">
	<div class="mb-4 flex items-center justify-between">
		<h3 class="text-sm font-semibold text-ink-200 flex items-center gap-2">
			<CheckCircle size={16} class="text-emerald-400" />
			最近读完
		</h3>
		<a href="/reading" class="group inline-flex items-center gap-1 text-xs text-ink-400 hover:text-accent-400 transition-colors">
			更多
			<ArrowRight size={12} class="transition-transform group-hover:translate-x-0.5" />
		</a>
	</div>

	{#if loading}
		<div class="space-y-3">
			{#each Array(3) as _}
				<div class="flex gap-3 animate-pulse">
					<div class="h-12 w-9 rounded bg-ink-800/50"></div>
					<div class="flex-1 space-y-1.5 py-1">
						<div class="h-3 w-3/4 rounded bg-ink-800/50"></div>
						<div class="h-2 w-1/2 rounded bg-ink-800/30"></div>
					</div>
				</div>
			{/each}
		</div>
	{:else if books.length === 0}
		<p class="text-sm text-ink-500 py-4 text-center">还没有读完的书</p>
	{:else}
		<div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-3">
			{#each books as book (book.id)}
				<a href="/library/{book.id}" class="group flex flex-col rounded-lg overflow-hidden hover:ring-1 hover:ring-accent-500/30 transition-all">
					<div class="aspect-[3/4] w-full rounded-lg overflow-hidden bg-ink-800">
						{#if book.cover_path}
							<img src="/api/covers/{book.cover_path}" alt="" class="h-full w-full object-cover group-hover:scale-105 transition-transform duration-300" loading="lazy" />
						{:else}
							<div class="h-full w-full flex items-center justify-center text-2xl text-ink-600">📖</div>
						{/if}
					</div>
					<div class="mt-2 px-0.5">
						<p class="text-xs text-ink-200 truncate group-hover:text-accent-400 transition-colors font-medium">{book.title}</p>
						<p class="text-[10px] text-ink-500 truncate">{book.author ?? '未知作者'}</p>
					</div>
				</a>
			{/each}
		</div>
	{/if}
</div>
