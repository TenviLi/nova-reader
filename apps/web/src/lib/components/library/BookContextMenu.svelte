<script lang="ts">
import { getErrorMessage } from '$lib/utils';
	import type { Book } from '$types/models';
	import { goto } from '$app/navigation';
	import { api } from '$services/api';
	import { toast } from 'svelte-sonner';
	import { BookOpen, FolderPlus, Trash2, RefreshCw, Star, MoreHorizontal } from 'lucide-svelte';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu';

	let { book, onAddToCollection, onDelete } = $props<{
		book: Book;
		onAddToCollection?: (bookId: string) => void;
		onDelete?: (bookId: string) => void;
	}>();

	async function handleReprocess() {
		try {
			await api.reprocessBook(book.id);
			toast.success('已加入处理队列');
		} catch (e: unknown) {
			toast.error(getErrorMessage(e) || '操作失败');
		}
	}

	async function handleDelete() {
		if (!confirm(`确定删除「${book.title}」？`)) return;
		try {
			await api.deleteBook(book.id);
			toast.success('已删除');
			onDelete?.(book.id);
		} catch (e: unknown) {
			toast.error(getErrorMessage(e) || '删除失败');
		}
	}
</script>

	<DropdownMenu.Root>
		<DropdownMenu.Trigger
			aria-label={`打开《${book.title}》操作菜单`}
			class="rounded-md bg-ink-700/80 p-1.5 text-ink-300 transition-colors hover:text-ink-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
		>
			<MoreHorizontal size={14} strokeWidth={2} />
		</DropdownMenu.Trigger>
	<DropdownMenu.Content class="w-48 bg-ink-900 border-ink-800/60" align="end">
		<DropdownMenu.Item class="text-ink-300 hover:bg-ink-800/50 hover:text-ink-100 cursor-pointer" onclick={() => goto(`/reading/${book.id}`)}>
			<BookOpen size={14} class="mr-2" />
			开始阅读
		</DropdownMenu.Item>
		<DropdownMenu.Item class="text-ink-300 hover:bg-ink-800/50 hover:text-ink-100 cursor-pointer" onclick={() => onAddToCollection?.(book.id)}>
			<FolderPlus size={14} class="mr-2" />
			添加到合集
		</DropdownMenu.Item>
		<DropdownMenu.Item class="text-ink-300 hover:bg-ink-800/50 hover:text-ink-100 cursor-pointer" onclick={() => goto(`/library/${book.id}`)}>
			<Star size={14} class="mr-2" />
			查看详情
		</DropdownMenu.Item>
		<DropdownMenu.Separator class="bg-ink-800/40" />
		<DropdownMenu.Item class="text-ink-300 hover:bg-ink-800/50 hover:text-ink-100 cursor-pointer" onclick={handleReprocess}>
			<RefreshCw size={14} class="mr-2" />
			重新处理
		</DropdownMenu.Item>
		<DropdownMenu.Separator class="bg-ink-800/40" />
		<DropdownMenu.Item class="text-red-400 hover:bg-red-500/10 cursor-pointer" onclick={handleDelete}>
			<Trash2 size={14} class="mr-2" />
			删除
		</DropdownMenu.Item>
	</DropdownMenu.Content>
</DropdownMenu.Root>
