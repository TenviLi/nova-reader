<script lang="ts">
	import { api } from '$services/api';
	import { toast } from 'svelte-sonner';
	import { PenSquare, Languages, Search, Copy } from 'lucide-svelte';

	let { visible, x, y, selectedText, bookId, chapterIndex, onAnnotate, onClose } = $props<{
		visible: boolean;
		x: number;
		y: number;
		selectedText: string;
		bookId: string;
		chapterIndex: number;
		onAnnotate: (note: string, color: string) => void;
		onClose: () => void;
	}>();

	let translating = $state(false);
	let translation = $state('');
	let lookupResult = $state('');
	let showAnnotateInput = $state(false);
	let annotationNote = $state('');
	let annotationColor = $state('#fef08a');

	const colors = ['#fef08a', '#bbf7d0', '#bfdbfe', '#fbcfe8', '#e9d5ff', '#fed7aa'];

	async function translateSelection() {
		translating = true;
		try {
			const result = await api.translate({
				text: selectedText,
				source_language: 'zh',
				target_language: 'en',
				book_id: bookId,
				use_glossary: true,
			});
			translation = result.translated_text;
		} catch (e: unknown) {
			toast.error('翻译失败');
		} finally {
			translating = false;
		}
	}

	async function lookupEntity() {
		try {
			const entities = await api.getEntities({ search: selectedText, limit: 1 });
			if (entities.length > 0) {
				lookupResult = `${entities[0].name}: ${entities[0].description}`;
			} else {
				lookupResult = '未找到相关实体';
			}
		} catch {
			lookupResult = '查询失败';
		}
	}

	function copyText() {
		navigator.clipboard.writeText(selectedText);
		toast.success('已复制');
		onClose();
	}

	function submitAnnotation() {
		onAnnotate(annotationNote, annotationColor);
		showAnnotateInput = false;
		annotationNote = '';
		onClose();
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') onClose();
	}

	let menuRef: HTMLDivElement | undefined = $state();

	$effect(() => {
		if (visible && menuRef) {
			// Focus the first button when menu appears
			const first = menuRef.querySelector('button') as HTMLElement;
			first?.focus();
		}
	});
</script>

<svelte:window onkeydown={handleKeydown} />

{#if visible}
	<!-- Backdrop -->
	<div role="presentation" class="fixed inset-0 z-40" onclick={onClose}></div>

	<!-- Menu -->
	<div
		bind:this={menuRef}
		role="menu"
		aria-label="选中文本操作"
		class="fixed z-50 w-60 rounded-xl border border-ink-700/50 bg-ink-900 shadow-2xl overflow-hidden animate-scale-in"
		style="left: {x}px; top: {y}px;"
	>
		{#if !showAnnotateInput && !translation && !lookupResult}
			<!-- Main menu -->
			<div class="p-1.5" role="group">
				<button
					role="menuitem"
					onclick={() => showAnnotateInput = true}
					class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm text-ink-200 hover:bg-ink-800/50 transition-colors"
				>
					<PenSquare size={15} strokeWidth={2} class="text-ink-400" />
					添加批注
				</button>
				<button
					role="menuitem"
					onclick={translateSelection}
					class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm text-ink-200 hover:bg-ink-800/50 transition-colors"
				>
					<Languages size={15} strokeWidth={2} class="text-ink-400" />
					翻译
				</button>
				<button
					role="menuitem"
					onclick={lookupEntity}
					class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm text-ink-200 hover:bg-ink-800/50 transition-colors"
				>
					<Search size={15} strokeWidth={2} class="text-ink-400" />
					查询实体
				</button>
				<button
					role="menuitem"
					onclick={copyText}
					class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm text-ink-200 hover:bg-ink-800/50 transition-colors"
				>
					<Copy size={15} strokeWidth={2} class="text-ink-400" />
					复制
				</button>
			</div>
		{:else if showAnnotateInput}
			<!-- Annotation form -->
			<div class="p-3 space-y-3">
				<div class="text-xs text-ink-400 truncate">"{selectedText.slice(0, 40)}..."</div>
				<textarea
					bind:value={annotationNote}
					placeholder="添加笔记（可选）"
					rows="3"
					class="w-full resize-none rounded-lg border border-ink-700/50 bg-ink-800/50 px-3 py-2 text-sm text-ink-200 placeholder-ink-500 outline-none"
				></textarea>
				<div class="flex gap-1.5" role="radiogroup" aria-label="高亮颜色">
					{#each colors as c, i}
						{@const colorNames = ['黄色', '绿色', '蓝色', '粉色', '紫色', '橙色']}
						<button
							onclick={() => annotationColor = c}
							class="h-5 w-5 rounded-full border-2 transition-transform"
							class:border-ink-200={annotationColor === c}
							class:border-transparent={annotationColor !== c}
							class:scale-110={annotationColor === c}
							style="background: {c}"
							role="radio"
							aria-checked={annotationColor === c}
							aria-label={colorNames[i]}
						></button>
					{/each}
				</div>
				<div class="flex justify-end gap-2">
					<button onclick={() => { showAnnotateInput = false; }} class="px-3 py-1 text-xs text-ink-400">返回</button>
					<button onclick={submitAnnotation} class="px-3 py-1 rounded-md bg-accent-500 text-xs font-medium text-ink-950">保存</button>
				</div>
			</div>
		{:else if translation}
			<!-- Translation result -->
			<div class="p-3 space-y-2">
				<div class="text-xs text-ink-500">翻译结果:</div>
				<p class="text-sm text-ink-200">{translation}</p>
				<button onclick={() => { translation = ''; }} class="text-xs text-ink-400 hover:text-accent-400">← 返回</button>
			</div>
		{:else if lookupResult}
			<!-- Entity lookup result -->
			<div class="p-3 space-y-2">
				<div class="text-xs text-ink-500">实体信息:</div>
				<p class="text-sm text-ink-200">{lookupResult}</p>
				<button onclick={() => { lookupResult = ''; }} class="text-xs text-ink-400 hover:text-accent-400">← 返回</button>
			</div>
		{/if}
	</div>
{/if}
