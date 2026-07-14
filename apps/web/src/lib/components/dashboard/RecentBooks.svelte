<script lang="ts">
	import { api } from '$services/api';
	import { onMount } from 'svelte';
	import { BookOpen, Plus, ArrowRight } from 'lucide-svelte';

	interface BookItem {
		id: string;
		title: string;
		author?: string | null;
		cover_path?: string | null;
		progress?: number;
		updated_at: string;
	}

	let books = $state<BookItem[]>([]);
	let loading = $state(true);

	onMount(async () => {
		try {
			const res = await api.getBooks({ sort_by: 'updated_at', per_page: 6 });
			books = res.data;
		} catch {
			// fallback gracefully
		} finally {
			loading = false;
		}
	});
</script>

<div class="rounded-xl border border-ink-800/40 bg-ink-900/30 p-5">
	<div class="mb-4 flex items-center justify-between">
		<h3 class="text-lg font-semibold text-ink-100">最近阅读</h3>
		<a href="/library" class="group inline-flex items-center gap-1 text-sm text-ink-400 hover:text-accent-400 transition-colors">
			查看全部
			<ArrowRight size={14} class="transition-transform group-hover:translate-x-0.5" />
		</a>
	</div>

	{#if loading}
		<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
			{#each Array(6) as _}
				<div class="flex gap-3 rounded-lg p-2">
					<div class="h-20 w-14 shrink-0 rounded-md bg-ink-800/50 animate-pulse"></div>
					<div class="flex-1 space-y-2 py-2">
						<div class="h-3 w-3/4 rounded bg-ink-800/50 animate-pulse"></div>
						<div class="h-2 w-1/2 rounded bg-ink-800/30 animate-pulse"></div>
					</div>
				</div>
			{/each}
		</div>
	{:else if books.length === 0}
		<div class="flex flex-col items-center justify-center py-12 text-center">
			<div class="mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-ink-800/50 ring-1 ring-ink-700/50">
				<BookOpen class="h-7 w-7 text-ink-500" strokeWidth={1.5} />
			</div>
			<p class="text-ink-400">还没有添加书籍</p>
			<p class="mt-1 text-sm text-ink-500">添加书库后会自动扫描新书</p>
			<a
				href="/libraries"
				class="mt-4 inline-flex items-center gap-2 rounded-lg bg-accent-500/10 px-4 py-2 text-sm font-medium text-accent-400 hover:bg-accent-500/20 transition-colors"
			>
				<Plus size={15} strokeWidth={2} />
				添加书库
			</a>
		</div>
	{:else}
		<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
			{#each books as book}
				<a href="/reading/{book.id}" class="group flex gap-3 rounded-lg p-2 transition-colors hover:bg-ink-800/30">
					<!-- Cover -->
					<div class="h-20 w-14 shrink-0 overflow-hidden rounded-md bg-ink-800 shadow-md">
						{#if book.cover_path}
							<img src={book.cover_path} alt={book.title} loading="lazy" class="h-full w-full object-cover" />
						{:else}
							<div class="flex h-full items-center justify-center text-[10px] text-ink-500 px-1 text-center leading-tight">
								{book.title}
							</div>
						{/if}
					</div>

					<!-- Info -->
					<div class="flex flex-1 flex-col justify-center overflow-hidden">
						<span class="truncate text-sm font-medium text-ink-100 group-hover:text-accent-400 transition-colors">
							{book.title}
						</span>
						<span class="mt-0.5 truncate text-xs text-ink-400">{book.author ?? '未知作者'}</span>
						<!-- Progress bar -->
						<div class="mt-2 h-1 w-full overflow-hidden rounded-full bg-ink-800">
							<div
								class="h-full rounded-full bg-accent-500 transition-all"
								style="width: {(book.progress ?? 0) * 100}%"
							></div>
						</div>
					</div>
				</a>
			{/each}
		</div>
	{/if}
</div>
