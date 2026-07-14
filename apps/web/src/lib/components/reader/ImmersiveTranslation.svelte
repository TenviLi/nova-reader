<script lang="ts">
import { getErrorMessage } from '$lib/utils';
	import { api } from '$services/api';
	import { toast } from 'svelte-sonner';

	/**
	 * Immersive Translation Component
	 * Inspired by 沉浸式翻译 (ImmerseTranslate)
	 *
	 * Modes:
	 * - bilingual: 原文 + 译文交替显示
	 * - translation-only: 仅显示译文
	 * - original-only: 仅显示原文 (默认)
	 * - hover: 鼠标悬停段落时显示译文 tooltip
	 */

	type TranslationMode = 'bilingual' | 'translation-only' | 'original-only' | 'hover';

	let { content, bookId, chapterIndex, sourceLanguage = 'zh', targetLanguage = 'en' } = $props<{
		content: string;
		bookId: string;
		chapterIndex: number;
		sourceLanguage?: string;
		targetLanguage?: string;
	}>();

	let mode = $state<TranslationMode>('original-only');
	let translating = $state(false);
	let translatedParagraphs = $state<Map<number, string>>(new Map());
	let hoveredParagraph = $state<number | null>(null);
	let translationProgress = $state(0);
	let glossaryTerms = $state<Array<{ source: string; target: string }>>([]);

	// Split content into paragraphs
	let paragraphs = $derived(
		content
			.split(/\n\n|\n/)
			.filter((p: string) => p.trim().length > 0)
			.map((text: string, index: number) => ({ text: text.trim(), index }))
	);

	// Translate visible paragraphs lazily (沉浸式翻译 style)
	async function translateAll() {
		if (translating) return;
		translating = true;
		translationProgress = 0;

		try {
			// Load glossary for term consistency
			const glossary = await api.getGlossary({ book_id: bookId }).catch(() => []);
			glossaryTerms = glossary.map((g) => ({ source: g.source_term, target: g.target_term }));

			// Batch translate paragraphs (5 at a time for rate limiting)
			const batchSize = 5;
			for (let i = 0; i < paragraphs.length; i += batchSize) {
				const batch = paragraphs.slice(i, i + batchSize);
				const texts = batch.map((p: { text: string; index: number }) => p.text).join('\n---PARA_SPLIT---\n');

				try {
					const result = await api.translate({
						text: texts,
						source_language: sourceLanguage,
						target_language: targetLanguage,
						book_id: bookId,
						use_glossary: true,
					});

					// Split translated text back into paragraphs
					const translatedTexts = result.translated_text.split(/---PARA_SPLIT---|\n---\n/);
					batch.forEach((p: { text: string; index: number }, idx: number) => {
						if (translatedTexts[idx]) {
							translatedParagraphs.set(p.index, translatedTexts[idx].trim());
						}
					});
					// Force reactivity
					translatedParagraphs = new Map(translatedParagraphs);
				} catch {
					// Continue with remaining paragraphs on error
				}

				translationProgress = Math.min(100, Math.round(((i + batchSize) / paragraphs.length) * 100));
			}

			toast.success('翻译完成');
		} catch (e: unknown) {
			toast.error(`翻译失败: ${getErrorMessage(e)}`);
		} finally {
			translating = false;
			translationProgress = 100;
		}
	}

	// Translate a single paragraph on demand (hover mode)
	async function translateParagraph(index: number) {
		if (translatedParagraphs.has(index)) return;

		const para = paragraphs.find((p: { index: number }) => p.index === index);
		if (!para) return;

		try {
			const result = await api.translate({
				text: para.text,
				source_language: sourceLanguage,
				target_language: targetLanguage,
				book_id: bookId,
				use_glossary: true,
			});
			translatedParagraphs.set(index, result.translated_text);
			translatedParagraphs = new Map(translatedParagraphs);
		} catch {
			// Silently fail for hover translation
		}
	}

	function handleParagraphHover(index: number) {
		if (mode !== 'hover') return;
		hoveredParagraph = index;
		translateParagraph(index);
	}

	// Highlight glossary terms in text
	function highlightGlossary(text: string): string {
		if (glossaryTerms.length === 0) return text;
		let result = text;
		for (const term of glossaryTerms) {
			const regex = new RegExp(`(${term.source.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`, 'g');
			result = result.replace(regex, `<span class="glossary-term" title="${term.target}">$1</span>`);
		}
		return result;
	}
</script>

<!-- Translation mode selector (floating toolbar) -->
<div class="fixed bottom-6 right-6 z-40 flex items-center gap-2 rounded-xl bg-ink-900/95 border border-ink-700/50 px-3 py-2 shadow-xl backdrop-blur-sm">
	<button
		onclick={() => { mode = 'original-only'; }}
		class="px-2.5 py-1 text-xs rounded-lg transition-all {mode === 'original-only' ? 'bg-accent-500/20 text-accent-400 font-medium' : 'text-ink-400 hover:text-ink-200'}"
	>
		原文
	</button>
	<button
		onclick={() => { mode = 'bilingual'; if (translatedParagraphs.size === 0) translateAll(); }}
		class="px-2.5 py-1 text-xs rounded-lg transition-all {mode === 'bilingual' ? 'bg-accent-500/20 text-accent-400 font-medium' : 'text-ink-400 hover:text-ink-200'}"
	>
		双语
	</button>
	<button
		onclick={() => { mode = 'translation-only'; if (translatedParagraphs.size === 0) translateAll(); }}
		class="px-2.5 py-1 text-xs rounded-lg transition-all {mode === 'translation-only' ? 'bg-accent-500/20 text-accent-400 font-medium' : 'text-ink-400 hover:text-ink-200'}"
	>
		译文
	</button>
	<button
		onclick={() => { mode = 'hover'; }}
		class="px-2.5 py-1 text-xs rounded-lg transition-all {mode === 'hover' ? 'bg-accent-500/20 text-accent-400 font-medium' : 'text-ink-400 hover:text-ink-200'}"
	>
		悬浮
	</button>

	<!-- Progress bar during translation -->
	{#if translating}
		<div class="ml-2 flex items-center gap-2">
			<div class="w-16 h-1.5 bg-ink-800 rounded-full overflow-hidden">
				<div class="h-full bg-accent-500 rounded-full transition-all duration-300" style="width: {translationProgress}%"></div>
			</div>
			<span class="text-[10px] text-ink-500">{translationProgress}%</span>
		</div>
	{/if}
</div>

<!-- Bilingual content -->
<div class="immersive-translate space-y-1">
	{#each paragraphs as para (para.index)}
		<div
			role="group"
			aria-label="段落 {para.index + 1}"
			class="para-group relative"
			onmouseenter={() => handleParagraphHover(para.index)}
			onmouseleave={() => hoveredParagraph = null}
		>
			<!-- Original text -->
			{#if mode !== 'translation-only'}
				<p class="original-text text-ink-200 leading-relaxed {mode === 'bilingual' ? 'mb-1' : ''}">
					{@html highlightGlossary(para.text)}
				</p>
			{/if}

			<!-- Translated text (bilingual mode) -->
			{#if mode === 'bilingual' && translatedParagraphs.has(para.index)}
				<p class="translated-text text-[0.9em] text-ink-400 italic border-l-2 border-accent-500/30 pl-3 mb-3 leading-relaxed">
					{translatedParagraphs.get(para.index)}
				</p>
			{/if}

			<!-- Translation only mode -->
			{#if mode === 'translation-only'}
				{#if translatedParagraphs.has(para.index)}
					<p class="translated-text text-ink-200 leading-relaxed">
						{translatedParagraphs.get(para.index)}
					</p>
				{:else}
					<p class="text-ink-600 italic leading-relaxed">翻译中...</p>
				{/if}
			{/if}

			<!-- Hover tooltip (hover mode) -->
			{#if mode === 'hover' && hoveredParagraph === para.index && translatedParagraphs.has(para.index)}
				<div class="absolute left-0 right-0 -bottom-2 translate-y-full z-30 p-3 rounded-lg bg-ink-800/95 border border-ink-700/50 shadow-xl text-sm text-ink-300 italic animate-fade-in">
					{translatedParagraphs.get(para.index)}
				</div>
			{/if}
		</div>
	{/each}
</div>

<style>
	:global(.glossary-term) {
		border-bottom: 1px dashed rgba(var(--color-accent-500), 0.6);
		cursor: help;
	}
	:global(.glossary-term:hover) {
		background-color: rgba(var(--color-accent-500), 0.1);
	}
	.para-group {
		padding: 0.25rem 0;
	}
	.animate-fade-in {
		animation: fadeIn 0.15s ease-out;
	}
	@keyframes fadeIn {
		from { opacity: 0; transform: translateY(calc(-100% + 4px)); }
		to { opacity: 1; transform: translateY(-100%); }
	}
</style>
