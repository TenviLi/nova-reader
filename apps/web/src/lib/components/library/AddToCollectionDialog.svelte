<script lang="ts">
import { getErrorMessage } from '$lib/utils';
	import { api } from '$services/api';
	import { onMount } from 'svelte';
	import { toast } from 'svelte-sonner';
	import { FolderPlus, Plus, Check } from 'lucide-svelte';
	import * as Dialog from '$lib/components/ui/dialog';
	import Button from '$lib/components/ui/button/button.svelte';
	import Input from '$lib/components/ui/input/input.svelte';

	import type { Collection } from '$types/models';

	let { open = $bindable(false), bookIds = [] } = $props<{
		open: boolean;
		bookIds: string[];
	}>();

	let collections = $state<Collection[]>([]);
	let loading = $state(true);
	let creating = $state(false);
	let newName = $state('');
	let showCreateForm = $state(false);
	let addingTo = $state<string | null>(null);
	let addedTo = $state<Set<string>>(new Set());

	onMount(loadCollections);

	$effect(() => {
		if (open) {
			loadCollections();
			addedTo = new Set();
		}
	});

	async function loadCollections() {
		loading = true;
		try {
			collections = await api.getCollections();
		} catch {
			collections = [];
		} finally {
			loading = false;
		}
	}

	async function addToCollection(collectionId: string) {
		addingTo = collectionId;
		try {
			for (const bookId of bookIds) {
				await api.addBookToCollection(collectionId, bookId);
			}
			addedTo = new Set([...addedTo, collectionId]);
			const name = collections.find(c => c.id === collectionId)?.name ?? '合集';
			toast.success(`已添加 ${bookIds.length} 本书到「${name}」`);
		} catch (e: unknown) {
			toast.error(getErrorMessage(e) || '添加失败');
		} finally {
			addingTo = null;
		}
	}

	async function createAndAdd() {
		if (!newName.trim()) return;
		creating = true;
		try {
			const col = await api.createCollection({ name: newName.trim() });
			collections = [...collections, col];
			newName = '';
			showCreateForm = false;
			await addToCollection(col.id);
		} catch (e: unknown) {
			toast.error(getErrorMessage(e) || '创建失败');
		} finally {
			creating = false;
		}
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-md bg-ink-950 border-ink-800">
		<Dialog.Header>
			<Dialog.Title class="text-ink-50 flex items-center gap-2">
				<FolderPlus class="w-5 h-5 text-accent-400" />
				添加到合集
			</Dialog.Title>
			<Dialog.Description class="text-ink-400">
				{bookIds.length > 1 ? `将 ${bookIds.length} 本书添加到合集` : '选择一个合集或创建新合集'}
			</Dialog.Description>
		</Dialog.Header>

		<div class="max-h-[300px] overflow-y-auto py-2 space-y-1">
			{#if loading}
				{#each Array(3) as _}
					<div class="h-12 rounded-lg bg-ink-900 animate-pulse"></div>
				{/each}
			{:else if collections.length === 0 && !showCreateForm}
				<p class="text-center text-sm text-ink-500 py-6">还没有合集，创建一个吧</p>
			{:else}
				{#each collections as col}
					{@const added = addedTo.has(col.id)}
					<button
						class="flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left transition-colors {added ? 'bg-emerald-500/10 border border-emerald-500/30' : 'hover:bg-ink-800/60 border border-transparent'}"
						disabled={addingTo !== null || added}
						onclick={() => addToCollection(col.id)}
					>
						<div class="flex h-8 w-8 shrink-0 items-center justify-center rounded-md {added ? 'bg-emerald-500/20 text-emerald-400' : 'bg-ink-800 text-ink-400'}">
							{#if added}
								<Check class="w-4 h-4" />
							{:else if addingTo === col.id}
								<div class="w-4 h-4 border-2 border-accent-400 border-t-transparent rounded-full animate-spin"></div>
							{:else}
								<FolderPlus class="w-4 h-4" />
							{/if}
						</div>
						<div class="flex-1 min-w-0">
							<p class="text-sm font-medium text-ink-100 truncate">{col.name}</p>
							{#if col.description}
								<p class="text-xs text-ink-500 truncate">{col.description}</p>
							{/if}
						</div>
						{#if col.book_count != null}
							<span class="text-xs text-ink-600 tabular-nums">{col.book_count} 本</span>
						{/if}
					</button>
				{/each}
			{/if}
		</div>

		<!-- Create new collection inline -->
		{#if showCreateForm}
			<form class="flex items-center gap-2 pt-2 border-t border-ink-800/50" onsubmit={(e) => { e.preventDefault(); createAndAdd(); }}>
				<Input
					bind:value={newName}
					placeholder="新合集名称"
					class="flex-1 bg-ink-900 border-ink-700 text-ink-100 placeholder:text-ink-600"
					autofocus
				/>
				<Button size="sm" disabled={creating || !newName.trim()} type="submit" class="bg-accent-500 text-ink-950 hover:bg-accent-400">
					{#if creating}
						<div class="w-3.5 h-3.5 border-2 border-ink-950 border-t-transparent rounded-full animate-spin"></div>
					{:else}
						创建
					{/if}
				</Button>
				<Button size="sm" variant="ghost" onclick={() => showCreateForm = false} class="text-ink-400">
					取消
				</Button>
			</form>
		{:else}
			<div class="pt-2 border-t border-ink-800/50">
				<Button variant="outline" size="sm" onclick={() => showCreateForm = true} class="w-full border-ink-700 text-ink-300 hover:text-ink-100 hover:border-ink-600">
					<Plus class="w-4 h-4 mr-2" />
					新建合集
				</Button>
			</div>
		{/if}
	</Dialog.Content>
</Dialog.Root>
