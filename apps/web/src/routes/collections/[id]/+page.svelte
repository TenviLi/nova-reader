<script lang="ts">
	import { page } from '$app/stores';
	import { api } from '$services/api';
	import type { Collection, Book } from '$types/models';
	import { ChevronLeft, PenSquare, X } from 'lucide-svelte';
	import BookCard from '$components/library/BookCard.svelte';

	let collection = $state<Collection | null>(null);
	let books = $state<Book[]>([]);
	let loading = $state(true);
	let editMode = $state(false);
	let editName = $state('');
	let editDescription = $state('');

	const collectionId = $derived($page.params.id);

	$effect(() => {
		if (collectionId) loadCollection(collectionId);
	});

	async function loadCollection(id: string) {
		loading = true;
		try {
			const [col, colBooks] = await Promise.all([
				api.getCollection(id),
				api.getCollectionBooks(id),
			]);
			collection = col;
			books = colBooks;
		} finally {
			loading = false;
		}
	}

	function startEdit() {
		if (!collection) return;
		editName = collection.name;
		editDescription = collection.description ?? '';
		editMode = true;
	}

	async function saveEdit() {
		if (!collection || !editName.trim()) return;
		await api.updateCollection(collection.id, { name: editName, description: editDescription || undefined });
		collection = { ...collection, name: editName, description: editDescription || null };
		editMode = false;
	}

	async function removeBook(bookId: string) {
		if (!collection) return;
		await api.removeBookFromCollection(collection.id, bookId);
		books = books.filter(b => b.id !== bookId);
	}
</script>

<svelte:head>
	<title>{collection?.name ?? '合集'} - Nova Reader</title>
</svelte:head>

<div class="p-6 max-w-5xl mx-auto">
	{#if loading}
		<div class="animate-pulse space-y-4">
			<div class="h-8 w-64 bg-parchment-200 rounded"></div>
			<div class="h-4 w-96 bg-parchment-100 rounded"></div>
			<div class="grid grid-cols-4 gap-4 mt-8">
				{#each Array(8) as _}
					<div class="h-56 bg-parchment-100 rounded-xl"></div>
				{/each}
			</div>
		</div>
	{:else if collection}
		<!-- Header -->
		<div class="mb-8">
			<div class="flex items-center gap-2 mb-4">
				<a href="/collections" class="text-ink-400 hover:text-ink-600">
					<ChevronLeft size={20} strokeWidth={2} />
				</a>
				<span class="text-sm text-ink-400">合集</span>
			</div>

			{#if editMode}
				<div class="space-y-3">
					<input
						bind:value={editName}
						class="w-full text-2xl font-bold px-3 py-1 border border-parchment-300 rounded-lg focus:ring-2 focus:ring-accent-500"
					/>
					<textarea
						bind:value={editDescription}
						rows="2"
						class="w-full px-3 py-2 border border-parchment-300 rounded-lg focus:ring-2 focus:ring-accent-500 resize-none text-sm"
						placeholder="描述"
					></textarea>
					<div class="flex gap-2">
						<button onclick={saveEdit} class="px-3 py-1.5 bg-accent-600 text-white rounded-lg text-sm hover:bg-accent-700">保存</button>
						<button onclick={() => editMode = false} class="px-3 py-1.5 text-ink-600 hover:bg-parchment-100 rounded-lg text-sm">取消</button>
					</div>
				</div>
			{:else}
				<div class="flex items-start justify-between">
					<div>
						<h1 class="text-2xl font-bold text-ink-900">{collection.name}</h1>
						{#if collection.description}
							<p class="text-ink-500 mt-1">{collection.description}</p>
						{/if}
						<p class="text-sm text-ink-400 mt-2">{books.length} 本书</p>
					</div>
					<button
						onclick={startEdit}
						class="p-2 text-ink-400 hover:text-ink-600 hover:bg-parchment-100 rounded-lg transition-colors"
					>
						<PenSquare size={20} strokeWidth={2} />
					</button>
				</div>
			{/if}
		</div>

		<!-- Books grid -->
		{#if books.length === 0}
			<div class="text-center py-16">
				<p class="text-ink-400 mb-2">此合集还没有书籍</p>
				<a href="/library" class="text-accent-600 hover:underline text-sm">从书库添加</a>
			</div>
		{:else}
			<div class="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-4">
				{#each books as book (book.id)}
					<div class="relative">
						<BookCard {book} />
						<button
							onclick={() => removeBook(book.id)}
							class="absolute top-2 right-2 z-20 opacity-0 group-hover:opacity-100 p-1.5 bg-ink-900/90 rounded-full shadow text-ink-400 hover:text-red-500 transition-all backdrop-blur-sm"
							title="从合集中移除"
						>
							<X size={14} strokeWidth={2} />
						</button>
					</div>
				{/each}
			</div>
		{/if}
	{:else}
		<div class="text-center py-20">
			<p class="text-ink-500">未找到该合集</p>
			<a href="/collections" class="mt-4 inline-block text-accent-600 hover:underline">返回合集列表</a>
		</div>
	{/if}
</div>
