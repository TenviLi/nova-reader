<script lang="ts">
	import type { Book } from '$types/models';
	import BookContextMenu from './BookContextMenu.svelte';

	type BookCardVariant = 'cover' | 'compact' | 'feature';

	let {
		book,
		variant = 'cover',
		showProgress = true,
		showBadge = true,
		showFormat = true,
		showActions = true,
		selectionMode = false,
		selected = false,
		eyebrow,
		href,
		readHref,
		onSelect,
		onAddToCollection,
		onDelete,
	} = $props<{
		book: Book;
		variant?: BookCardVariant;
		showProgress?: boolean;
		showBadge?: boolean;
		showFormat?: boolean;
		showActions?: boolean;
		selectionMode?: boolean;
		selected?: boolean;
		eyebrow?: string;
		href?: string;
		readHref?: string;
		onSelect?: (id: string) => void;
		onAddToCollection?: (id: string) => void;
		onDelete?: (id: string) => void;
	}>();

	let isNew = $derived(Date.now() - new Date(book.created_at).getTime() < 7 * 86400000);
	let progressPercent = $derived(Math.round((book.progress ?? 0) * 100));
	let detailHref = $derived(href ?? `/library/${book.id}`);
	let readingHref = $derived(readHref ?? `/reading/${book.id}`);
	let coverSrc = $derived(normalizeCoverPath(book.cover_path));

	function normalizeCoverPath(path: string | null | undefined): string | null {
		if (!path) return null;
		if (path.startsWith('/api/') || path.startsWith('http') || path.startsWith('data:')) return path;
		return `/api/covers/${path}`;
	}
</script>

<div class="group relative" class:rounded-xl={variant !== 'cover'}>
	{#if selectionMode}
		<div class="absolute left-2 top-2 z-30">
			<input
				data-book-checkbox
				type="checkbox"
				checked={selected}
				aria-label={`选择 ${book.title}`}
				onclick={(e) => e.stopPropagation()}
				onchange={() => onSelect?.(book.id)}
				class="h-5 w-5 rounded border-2 border-white/70 bg-ink-900/70 accent-accent-500 backdrop-blur-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
			/>
		</div>
	{/if}

	{#if variant === 'compact'}
		<div
			class="relative flex h-full min-h-28 gap-3 rounded-xl border border-ink-800/50 bg-gradient-to-br from-ink-900/80 via-ink-900/55 to-ink-800/30 p-3 transition-transform transition-colors hover:-translate-y-0.5 hover:border-accent-500/25 hover:bg-ink-900 hover:shadow-lg hover:shadow-ink-950/20"
			class:ring-1={selected}
			class:ring-accent-500={selected}
		>
			<a
				href={detailHref}
				aria-label={`打开 ${book.title}`}
				class="absolute inset-0 z-0 rounded-xl focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
			></a>

			<div class="pointer-events-none relative z-10 h-24 w-16 shrink-0 overflow-hidden rounded-lg bg-ink-800 shadow-md ring-1 ring-white/5">
				{#if coverSrc}
					<img src={coverSrc} alt={book.title} class="h-full w-full object-cover" loading="lazy" />
				{:else}
					<div class="flex h-full flex-col items-center justify-center bg-gradient-to-br from-ink-800 to-ink-950 px-2 text-center">
						<span class="line-clamp-3 text-[10px] font-semibold leading-tight text-ink-300">{book.title}</span>
					</div>
				{/if}
			</div>

			<div class="pointer-events-none relative z-10 flex min-w-0 flex-1 flex-col justify-center">
				{#if eyebrow}
					<span class="mb-1 text-[10px] font-medium uppercase tracking-wide text-accent-400/80">{eyebrow}</span>
				{/if}
				<h4 class="line-clamp-2 text-sm font-semibold leading-snug text-ink-100 transition-colors group-hover:text-accent-300">
					{book.title}
				</h4>
				<p class="mt-1 truncate text-xs text-ink-400">
					{book.author ?? '未知作者'}
					{#if book.word_count}
						<span class="text-ink-600"> · {Math.round(book.word_count / 10000)}万字</span>
					{/if}
				</p>

				{#if showProgress}
					<div class="mt-3 flex items-center gap-2">
						<div class="h-1.5 flex-1 overflow-hidden rounded-full bg-ink-800/80">
							<div class="h-full rounded-full bg-accent-500 transition-[width]" style="width: {progressPercent}%"></div>
						</div>
						<span class="w-8 text-right text-[10px] tabular-nums text-ink-500">{progressPercent}%</span>
					</div>
				{/if}
			</div>

			{#if showActions}
				<div class="relative z-20 flex shrink-0 items-center gap-1 self-start">
					<a
						href={readingHref}
						class="rounded-md bg-accent-500 px-2.5 py-1 text-[11px] font-medium text-ink-950 transition-colors hover:bg-accent-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-300/70"
					>
						{book.progress > 0 ? '继续' : '阅读'}
					</a>
					{#if onAddToCollection || onDelete}
						<BookContextMenu {book} {onAddToCollection} {onDelete} />
					{/if}
				</div>
			{/if}
		</div>
	{:else if variant === 'feature'}
		<div class="relative flex min-h-40 overflow-hidden rounded-xl border border-ink-800/50 bg-gradient-to-br from-ink-900/90 via-ink-900/60 to-accent-950/20 p-4 transition-transform transition-colors hover:-translate-y-0.5 hover:border-accent-500/25 hover:shadow-xl hover:shadow-accent-950/10">
			<a
				href={detailHref}
				aria-label={`打开 ${book.title}`}
				class="absolute inset-0 z-0 rounded-xl focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
			></a>
			<div class="pointer-events-none absolute inset-x-0 bottom-0 z-10 h-24 bg-gradient-to-t from-accent-500/10 to-transparent opacity-80"></div>
			<div class="pointer-events-none relative z-10 h-32 w-[5.5rem] shrink-0 overflow-hidden rounded-lg bg-ink-800 shadow-lg ring-1 ring-white/5">
				{#if coverSrc}
					<img src={coverSrc} alt={book.title} class="h-full w-full object-cover" loading="lazy" />
				{:else}
					<div class="flex h-full items-center justify-center bg-gradient-to-br from-ink-800 to-ink-950 p-2 text-center text-xs font-semibold text-ink-300">{book.title}</div>
				{/if}
			</div>
			<div class="pointer-events-none relative z-10 ml-4 flex min-w-0 flex-1 flex-col justify-center">
				{#if eyebrow}
					<span class="mb-2 text-[10px] font-medium uppercase tracking-wide text-accent-400/80">{eyebrow}</span>
				{/if}
				<h4 class="line-clamp-2 text-base font-semibold leading-snug text-ink-50 transition-colors group-hover:text-accent-300">{book.title}</h4>
				<p class="mt-1 truncate text-sm text-ink-400">{book.author ?? '未知作者'}</p>
				{#if showProgress}
					<div class="mt-4 flex max-w-xs items-center gap-2">
						<div class="h-1.5 flex-1 overflow-hidden rounded-full bg-ink-800">
							<div class="h-full rounded-full bg-accent-500 transition-[width]" style="width: {progressPercent}%"></div>
						</div>
						<span class="text-[10px] tabular-nums text-ink-500">{progressPercent}%</span>
					</div>
				{/if}
			</div>
		</div>
	{:else}
		<div class="relative">
			<a
				href={detailHref}
				aria-label={`打开 ${book.title}`}
				class="absolute inset-0 z-0 rounded-xl focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
			></a>

			<div class="relative aspect-[2/3]">
				<div
					class="pointer-events-none relative z-10 h-full overflow-hidden rounded-xl bg-ink-800/50 shadow-md transition-transform duration-200 group-hover:-translate-y-1 group-hover:scale-[1.02] group-hover:shadow-xl group-hover:shadow-accent-500/5"
					class:ring-2={selected}
					class:ring-accent-500={selected}
				>
					{#if coverSrc}
						<img
							src={coverSrc}
							alt={book.title}
							class="h-full w-full object-cover"
							loading="lazy"
						/>
					{:else}
						<div class="flex h-full flex-col items-center justify-center bg-gradient-to-br from-ink-800 to-ink-900 p-4 text-center">
							<span class="text-sm font-bold leading-tight text-ink-300">{book.title}</span>
							{#if book.author}
								<span class="mt-2 text-xs text-ink-500">{book.author}</span>
							{/if}
						</div>
					{/if}

					{#if showProgress && book.progress > 0 && book.progress < 1}
						<div class="absolute bottom-0 left-0 right-0 h-1 bg-black/30">
							<div class="h-full bg-accent-400 transition-[width]" style="width: {progressPercent}%"></div>
						</div>
					{/if}

					{#if showBadge}
						{#if book.reading_status === 'reading'}
							<div class="absolute top-2 rounded-md bg-accent-500/90 px-1.5 py-0.5 text-[10px] font-medium text-ink-950" class:left-2={!selectionMode} class:left-9={selectionMode}>
								在读
							</div>
						{:else if book.reading_status === 'completed'}
							<div class="absolute top-2 rounded-md bg-emerald-500/90 px-1.5 py-0.5 text-[10px] font-medium text-ink-950" class:left-2={!selectionMode} class:left-9={selectionMode}>
								已读
							</div>
						{:else if isNew}
							<div class="absolute top-2 rounded-md bg-emerald-500/90 px-1.5 py-0.5 text-[10px] font-medium text-ink-950" class:left-2={!selectionMode} class:left-9={selectionMode}>
								新
							</div>
						{/if}
					{/if}

					{#if showFormat && book.format}
						<div class="absolute right-2 top-2 rounded-md bg-ink-900/80 px-1.5 py-0.5 text-[9px] font-bold uppercase text-ink-400 backdrop-blur-sm">
							{book.format}
						</div>
					{/if}
				</div>

				{#if showActions}
					<div class="pointer-events-none absolute inset-0 z-20 flex items-end rounded-xl bg-gradient-to-t from-black/80 via-transparent to-transparent p-3 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100">
						<div class="pointer-events-auto flex w-full items-center gap-2">
							<a
								href={readingHref}
								class="flex-1 rounded-md bg-accent-500 py-1.5 text-center text-xs font-medium text-ink-950 transition-colors hover:bg-accent-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-300/70"
							>
								{book.progress > 0 ? '继续阅读' : '开始阅读'}
							</a>
							{#if onAddToCollection || onDelete}
								<BookContextMenu {book} {onAddToCollection} {onDelete} />
							{/if}
						</div>
					</div>
				{/if}
			</div>

			<div class="pointer-events-none relative z-10 mt-3 px-0.5">
				<h4 class="truncate text-sm font-medium text-ink-100 transition-colors group-hover:text-accent-400">
					{book.title}
				</h4>
				<p class="mt-0.5 truncate text-xs text-ink-400">
					{book.author ?? '未知作者'}
					{#if book.word_count}
						<span class="text-ink-600"> · {Math.round(book.word_count / 10000)}万字</span>
					{/if}
				</p>
			</div>
		</div>
	{/if}
</div>
