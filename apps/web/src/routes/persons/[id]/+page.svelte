<script lang="ts">
	import { page } from '$app/stores';
	import { createQuery } from '@tanstack/svelte-query';
	import { api } from '$services/api';
	import { BookOpen, ChevronLeft, PenLine, User } from 'lucide-svelte';

	const personId = $derived($page.params.id!);

	const personQuery = createQuery(() => ({
		queryKey: ['person', personId],
		queryFn: () => api.getPerson(personId),
		enabled: !!personId,
	}));

	const booksQuery = createQuery(() => ({
		queryKey: ['person-books', personId],
		queryFn: () => api.getPersonBooks(personId),
		enabled: !!personId,
	}));

	function roleLabel(role: string): string {
		switch (role) {
			case 'author': return '作者';
			case 'translator': return '译者';
			case 'editor': return '编辑';
			case 'illustrator': return '插画';
			default: return role;
		}
	}

	function wordCountLabel(count: number): string {
		if (!count) return '字数未知';
		return count >= 10000 ? `${(count / 10000).toFixed(1)}万字` : `${count}字`;
	}
</script>

<svelte:head>
	<title>{personQuery.data?.name ?? '创作者'} — Nova Reader</title>
</svelte:head>

<div class="mx-auto max-w-6xl space-y-6 px-4 py-6 sm:px-6 lg:px-8">
	<a
		href="/persons"
		class="inline-flex items-center gap-2 text-sm text-ink-500 transition-colors hover:text-ink-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-400/70"
	>
		<ChevronLeft size={16} aria-hidden="true" />
		返回创作者
	</a>

	{#if personQuery.isLoading}
		<div class="animate-pulse space-y-6">
			<div class="h-28 rounded-xl border border-ink-800/60 bg-ink-900/50"></div>
			<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
				{#each Array(6) as _}
					<div class="h-32 rounded-lg border border-ink-800/60 bg-ink-900/40"></div>
				{/each}
			</div>
		</div>
	{:else if personQuery.error}
		<section class="rounded-xl border border-ink-800/60 bg-ink-900/40 p-8 text-center">
			<User size={36} class="mx-auto mb-3 text-ink-600" aria-hidden="true" />
			<h1 class="text-lg font-semibold text-ink-200">未找到创作者</h1>
			<p class="mt-1 text-sm text-ink-500">可能没有权限查看，或相关书籍已被移除。</p>
		</section>
	{:else if personQuery.data}
		{@const person = personQuery.data}
		<section class="rounded-xl border border-ink-800/60 bg-ink-900/45 p-5">
			<div class="flex flex-col gap-5 sm:flex-row sm:items-start">
				<div class="flex h-20 w-20 shrink-0 items-center justify-center overflow-hidden rounded-full bg-ink-800 text-2xl font-bold text-amber-300">
					{#if person.avatar_path}
						<img src={person.avatar_path} alt={person.name} width="80" height="80" class="h-full w-full object-cover" />
					{:else}
						{person.name.slice(0, 1)}
					{/if}
				</div>
				<div class="min-w-0 flex-1">
					<div class="flex flex-wrap items-center gap-2">
						<h1 class="text-2xl font-bold text-ink-50">{person.name}</h1>
						{#each person.roles as role}
							<span class="rounded-md bg-amber-500/10 px-2 py-0.5 text-xs font-medium text-amber-300">
								{roleLabel(role)}
							</span>
						{/each}
					</div>
					{#if person.original_name && person.original_name !== person.name}
						<p class="mt-1 text-sm text-ink-500">{person.original_name}</p>
					{/if}
					<div class="mt-4 flex flex-wrap gap-4 text-sm text-ink-400">
						<span>{person.book_count} 本作品</span>
						<span>{wordCountLabel(person.total_word_count)}</span>
					</div>
					{#if person.biography}
						<p class="mt-4 max-w-3xl text-sm leading-6 text-ink-300">{person.biography}</p>
					{/if}
				</div>
			</div>
		</section>

		<section>
			<div class="mb-3 flex items-center justify-between gap-3">
				<div>
					<h2 class="text-lg font-semibold text-ink-100">关联作品</h2>
					<p class="mt-1 text-sm text-ink-500">按创作身份聚合当前可访问书库中的书籍。</p>
				</div>
			</div>

			{#if booksQuery.isLoading}
				<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
					{#each Array(6) as _}
						<div class="h-32 rounded-lg border border-ink-800/60 bg-ink-900/40 animate-pulse"></div>
					{/each}
				</div>
			{:else if (booksQuery.data?.length ?? 0) === 0}
				<div class="rounded-xl border border-dashed border-ink-800/70 bg-ink-900/25 p-8 text-center">
					<BookOpen size={32} class="mx-auto mb-3 text-ink-600" aria-hidden="true" />
					<p class="text-sm text-ink-400">暂无可访问作品</p>
				</div>
			{:else}
				<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
					{#each booksQuery.data ?? [] as book}
						<a
							href="/library/{book.id}"
							class="group flex min-w-0 gap-3 rounded-lg border border-ink-800/60 bg-ink-900/35 p-3 transition-colors hover:border-amber-500/30 hover:bg-ink-900/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-400/70"
						>
							<div class="h-20 w-14 shrink-0 overflow-hidden rounded bg-ink-800">
								{#if book.cover_path}
									<img src="/api/covers/{book.id}" alt={book.title} width="56" height="80" loading="lazy" class="h-full w-full object-cover" />
								{:else}
									<div class="flex h-full items-center justify-center px-1 text-center text-[10px] text-ink-500">
										{book.title.slice(0, 8)}
									</div>
								{/if}
							</div>
							<div class="min-w-0 flex-1">
								<h3 class="truncate text-sm font-semibold text-ink-100 group-hover:text-amber-300">{book.title}</h3>
								<p class="mt-1 text-xs text-ink-500">{roleLabel(book.role)}</p>
								<p class="mt-3 flex items-center gap-1 text-xs text-ink-500">
									<PenLine size={12} aria-hidden="true" />
									{wordCountLabel(book.word_count)}
								</p>
							</div>
						</a>
					{/each}
				</div>
			{/if}
		</section>
	{/if}
</div>
