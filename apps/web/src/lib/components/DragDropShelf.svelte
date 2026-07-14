<script lang="ts">
	import { GripVertical, BookOpen } from 'lucide-svelte';
	import { toast } from 'svelte-sonner';
	import { api } from '$services/api';

	let {
		shelfId = '',
		books = [],
		onreorder = (bookIds: string[]) => {},
	} = $props<{
		shelfId?: string;
		books?: Array<{ id: string; title: string; author?: string; cover_path?: string }>;
		onreorder?: (bookIds: string[]) => void;
	}>();

	type ShelfBook = { id: string; title: string; author?: string; cover_path?: string };

	let items = $state.raw<ShelfBook[]>([]);
	let dragIndex = $state<number | null>(null);
	let dropIndex = $state<number | null>(null);

	$effect(() => { items = books; });

	function handleDragStart(e: DragEvent, index: number) {
		dragIndex = index;
		if (e.dataTransfer) {
			e.dataTransfer.effectAllowed = 'move';
			e.dataTransfer.setData('text/plain', index.toString());
		}
	}

	function handleDragOver(e: DragEvent, index: number) {
		e.preventDefault();
		dropIndex = index;
	}

	function handleDrop(e: DragEvent, index: number) {
		e.preventDefault();
		if (dragIndex === null || dragIndex === index) {
			dragIndex = null;
			dropIndex = null;
			return;
		}

		// Reorder
		const newItems = [...items];
		const [moved] = newItems.splice(dragIndex, 1);
		newItems.splice(index, 0, moved);
		items = newItems;

		// Notify parent
		const bookIds = newItems.map(b => b.id);
		onreorder(bookIds);

		// Save to server
		saveOrder(bookIds);

		dragIndex = null;
		dropIndex = null;
	}

	function handleDragEnd() {
		dragIndex = null;
		dropIndex = null;
	}

	async function saveOrder(bookIds: string[]) {
		try {
			await api.reorderShelf(shelfId, bookIds);
		} catch {
			toast.error('排序保存失败');
		}
	}

	// Touch support
	let touchStartIndex = $state<number | null>(null);
	let touchY = $state(0);

	function handleTouchStart(e: TouchEvent, index: number) {
		touchStartIndex = index;
		touchY = e.touches[0].clientY;
	}

	function handleTouchMove(e: TouchEvent) {
		if (touchStartIndex === null) return;
		const currentY = e.touches[0].clientY;
		const elements = document.querySelectorAll('[data-shelf-item]');
		for (let i = 0; i < elements.length; i++) {
			const rect = elements[i].getBoundingClientRect();
			if (currentY >= rect.top && currentY <= rect.bottom) {
				dropIndex = i;
				break;
			}
		}
	}

	function handleTouchEnd() {
		if (touchStartIndex !== null && dropIndex !== null && touchStartIndex !== dropIndex) {
			const newItems = [...items];
			const [moved] = newItems.splice(touchStartIndex, 1);
			newItems.splice(dropIndex, 0, moved);
			items = newItems;
			const bookIds = newItems.map(b => b.id);
			onreorder(bookIds);
			saveOrder(bookIds);
		}
		touchStartIndex = null;
		dropIndex = null;
	}
</script>

<div
	class="space-y-1"
	role="list"
	aria-label="书架排序"
	ontouchmove={handleTouchMove}
	ontouchend={handleTouchEnd}
>
	{#each items as book, i (book.id)}
		<div
			data-shelf-item
			class="flex items-center gap-3 rounded-lg border px-3 py-2.5 cursor-grab active:cursor-grabbing transition-all
				{dragIndex === i ? 'opacity-50 border-accent-500' : 'border-ink-700 bg-ink-800/50'}
				{dropIndex === i && dragIndex !== i ? 'border-t-2 border-t-accent-400' : ''}"
			draggable="true"
			ondragstart={(e) => handleDragStart(e, i)}
			ondragover={(e) => handleDragOver(e, i)}
			ondrop={(e) => handleDrop(e, i)}
			ondragend={handleDragEnd}
			ontouchstart={(e) => handleTouchStart(e, i)}
			role="listitem"
		>
			<div class="text-ink-600 hover:text-ink-400 touch-none">
				<GripVertical size={16} />
			</div>

			{#if book.cover_path}
				<img
					src="/api/covers/{book.cover_path}"
					alt={book.title}
					class="w-8 h-11 rounded object-cover flex-shrink-0"
				/>
			{:else}
				<div class="w-8 h-11 rounded bg-ink-700 flex items-center justify-center flex-shrink-0">
					<BookOpen size={12} class="text-ink-500" />
				</div>
			{/if}

			<div class="flex-1 min-w-0">
				<p class="text-sm text-ink-200 truncate">{book.title}</p>
				{#if book.author}
					<p class="text-xs text-ink-500 truncate">{book.author}</p>
				{/if}
			</div>

			<span class="text-xs text-ink-600 font-mono w-6 text-right">{i + 1}</span>
		</div>
	{/each}
</div>
