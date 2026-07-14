<script lang="ts">
	import { getErrorMessage } from '$lib/utils';
	import { api } from '$services/api';
	import type { AppliedGlossaryMatch } from '$types/models';
	import { toast } from 'svelte-sonner';
	import { ArrowUpDown, Check, Columns2, Plus, Rows2, Sparkles, X } from 'lucide-svelte';

	let selectedBook = $state<string | null>(null);
	let selectedChapter = $state(0);
	let sourceLanguage = $state('zh');
	let targetLanguage = $state('en');
	let glossaryVisible = $state(true);
	let translating = $state(false);
	let compareMode = $state<'side' | 'aligned'>('side');
	let showGlossaryForm = $state(false);
	let savingGlossary = $state(false);
	let extractingGlossary = $state(false);
	let glossaryForm = $state({
		term: '',
		definition: '',
	});

	let sourceText = $state('');
	let translatedText = $state('');
	let glossaryApplied = $state.raw<AppliedGlossaryMatch[]>([]);
	let glossaryEntries = $state.raw<Array<{
		source: string;
		target: string;
		category: string;
		context: string;
	}>>([]);
	let extractedTerms = $state.raw<Array<{
		source: string;
		target: string;
		category: string;
		context: string;
	}>>([]);

	// Stats
	let charCount = $derived(sourceText.length);
	let wordCount = $derived(sourceText.replace(/\s/g, '').length);

	// Aligned sentence pairs for comparison view
	let alignedPairs = $derived.by(() => {
		if (!sourceText || !translatedText) return [];
		const srcSentences = splitSentences(sourceText);
		const tgtSentences = splitSentences(translatedText);
		const maxLen = Math.max(srcSentences.length, tgtSentences.length);
		const pairs: Array<{ source: string; target: string }> = [];
		for (let i = 0; i < maxLen; i++) {
			pairs.push({
				source: srcSentences[i] ?? '',
				target: tgtSentences[i] ?? '',
			});
		}
		return pairs;
	});

	function splitSentences(text: string): string[] {
		// Split by Chinese/English sentence-ending punctuation
		return text
			.split(/(?<=[。！？.!?\n])\s*/)
			.map(s => s.trim())
			.filter(s => s.length > 0);
	}

	const languages = [
		{ value: 'zh', label: '中文' },
		{ value: 'en', label: 'English' },
		{ value: 'ja', label: '日本語' },
		{ value: 'ko', label: '한국어' },
		{ value: 'fr', label: 'Français' },
		{ value: 'de', label: 'Deutsch' },
	];

	async function startTranslation() {
		if (!sourceText.trim()) return;
		translating = true;
		try {
			extractedTerms = [];
			glossaryApplied = [];
			const result = await api.translate({
				text: sourceText,
				source_language: sourceLanguage,
				target_language: targetLanguage,
				use_glossary: glossaryVisible,
			});
			translatedText = result.translated_text;
			glossaryApplied = result.glossary_applied;
			if (glossaryApplied.length > 0) {
				toast.success(`应用了 ${glossaryApplied.length} 条术语`);
			}
		} catch (e: unknown) {
			toast.error(`翻译失败: ${getErrorMessage(e)}`);
		} finally {
			translating = false;
		}
	}

	function buildGlossaryPairs() {
		const sourceUnits = splitSentences(sourceText);
		const targetUnits = splitSentences(translatedText);
		const maxLen = Math.min(sourceUnits.length, targetUnits.length, 20);
		const pairs: Array<{ source: string; target: string }> = [];
		for (let i = 0; i < maxLen; i++) {
			const source = sourceUnits[i]?.trim();
			const target = targetUnits[i]?.trim();
			if (source && target) pairs.push({ source, target });
		}
		return pairs;
	}

	async function extractTermsFromTranslation() {
		const pairs = buildGlossaryPairs();
		if (pairs.length === 0) {
			toast.error('请先完成一段原文和译文对照');
			return;
		}
		extractingGlossary = true;
		try {
			const result = await api.extractGlossary(pairs, sourceLanguage, targetLanguage);
			extractedTerms = result.terms ?? [];
			if (extractedTerms.length === 0) {
				toast.message('没有发现新的术语候选');
			} else {
				toast.success(`发现 ${extractedTerms.length} 个术语候选`);
			}
		} catch (e: unknown) {
			toast.error(`术语抽取失败: ${getErrorMessage(e)}`);
		} finally {
			extractingGlossary = false;
		}
	}

	async function loadGlossary() {
		try {
			const entries = await api.getGlossary({ book_id: selectedBook ?? undefined });
			glossaryEntries = entries.map(e => ({
				source: e.source_term,
				target: e.target_term,
				category: e.category ?? '通用',
				context: e.context ?? '',
			}));
		} catch { /* graceful fallback */ }
	}

	async function addGlossaryEntry() {
		const term = glossaryForm.term.trim();
		const definition = glossaryForm.definition.trim();
		if (!term || !definition || savingGlossary) return;
		savingGlossary = true;
		try {
			await api.createGlossaryEntry({
				term,
				definition,
				source_language: sourceLanguage,
				target_language: targetLanguage,
				book_id: selectedBook,
			});
			glossaryForm = { term: '', definition: '' };
			showGlossaryForm = false;
			await loadGlossary();
			toast.success('术语已添加');
		} catch (e: unknown) {
			toast.error(`术语添加失败: ${getErrorMessage(e)}`);
		} finally {
			savingGlossary = false;
		}
	}

	async function saveExtractedTerm(term: { source: string; target: string; category: string; context: string }) {
		try {
			await api.createGlossaryEntry({
				term: term.source,
				definition: term.target,
				source_language: sourceLanguage,
				target_language: targetLanguage,
				book_id: selectedBook,
			});
			extractedTerms = extractedTerms.filter(item => item.source !== term.source || item.target !== term.target);
			await loadGlossary();
			toast.success('术语已添加');
		} catch (e: unknown) {
			toast.error(`术语添加失败: ${getErrorMessage(e)}`);
		}
	}

	function copyTranslation() {
		navigator.clipboard.writeText(translatedText);
		toast.success('已复制到剪贴板');
	}

	$effect(() => {
		// Explicitly track selectedBook so glossary reloads when book changes
		const _book = selectedBook;
		loadGlossary();
	});
</script>

<svelte:head>
	<title>Nova Reader — 翻译工坊</title>
</svelte:head>

<div class="flex h-[calc(100vh-4rem)] overflow-hidden animate-fade-in">
	<!-- Main translation area -->
	<div class="flex flex-1 flex-col">
		<!-- Toolbar -->
		<div class="flex items-center justify-between border-b border-ink-800/50 px-5 py-3">
			<div class="flex items-center gap-3">
				<h1 class="text-lg font-semibold text-ink-100">翻译工坊</h1>
				<div class="flex items-center gap-2 text-sm">
					<select bind:value={sourceLanguage} class="rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-1.5 text-ink-200 outline-none">
						{#each languages as lang}
							<option value={lang.value}>{lang.label}</option>
						{/each}
					</select>
					<button
						type="button"
						onclick={() => { const tmp = sourceLanguage; sourceLanguage = targetLanguage; targetLanguage = tmp; }}
						aria-label="交换源语言和目标语言"
						class="rounded-md p-1.5 text-ink-400 transition-colors hover:text-accent-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
					>
					<ArrowUpDown size={16} strokeWidth={2} />
					</button>
					<select bind:value={targetLanguage} class="rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-1.5 text-ink-200 outline-none">
						{#each languages as lang}
							<option value={lang.value}>{lang.label}</option>
						{/each}
					</select>
				</div>
			</div>
			<div class="flex flex-wrap items-center justify-end gap-2">
				<!-- Compare mode toggle -->
				{#if translatedText}
					<div class="flex items-center rounded-lg border border-ink-700/50 overflow-hidden">
						<button
							type="button"
							onclick={() => compareMode = 'side'}
							aria-label="并排视图"
							class="px-2 py-1.5 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 {compareMode === 'side' ? 'bg-accent-500/15 text-accent-400' : 'text-ink-400 hover:text-ink-200'}"
							title="并排视图"
						>
							<Columns2 size={14} />
						</button>
						<button
							type="button"
							onclick={() => compareMode = 'aligned'}
							aria-label="逐句对照"
							class="px-2 py-1.5 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 {compareMode === 'aligned' ? 'bg-accent-500/15 text-accent-400' : 'text-ink-400 hover:text-ink-200'}"
							title="逐句对照"
						>
							<Rows2 size={14} />
						</button>
					</div>
					{/if}
					<button
						type="button"
						onclick={() => glossaryVisible = !glossaryVisible}
						aria-pressed={glossaryVisible}
						class="rounded-lg px-3 py-1.5 text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 {glossaryVisible ? 'bg-accent-500/10' : ''}"
						class:text-accent-400={glossaryVisible}
						class:text-ink-400={!glossaryVisible}
					>
						术语表
					</button>
					<button
						type="button"
						onclick={extractTermsFromTranslation}
						disabled={extractingGlossary || !sourceText.trim() || !translatedText.trim()}
						class="inline-flex items-center gap-2 rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-2 text-sm text-ink-300 transition-colors hover:border-accent-500/30 hover:text-accent-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 disabled:cursor-not-allowed disabled:opacity-50"
					>
						<Sparkles size={14} />
						{extractingGlossary ? '抽取中…' : '抽取术语'}
					</button>
					<button
						type="button"
						onclick={startTranslation}
						disabled={translating || !sourceText}
						class="inline-flex items-center gap-2 rounded-lg bg-accent-500 px-4 py-2 text-sm font-medium text-ink-950 transition-colors hover:bg-accent-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-300/80 disabled:opacity-50"
					>
					{#if translating}
						<div class="h-3.5 w-3.5 animate-spin rounded-full border-2 border-ink-950 border-t-transparent"></div>
					{/if}
					翻译
				</button>
			</div>
		</div>

		<!-- Translation panels -->
		<div class="flex flex-1 divide-x divide-ink-800/50 overflow-hidden">
			{#if compareMode === 'aligned' && translatedText}
				<!-- Aligned comparison view -->
				<div class="flex-1 overflow-y-auto p-4 space-y-1">
					{#each alignedPairs as pair, i}
						<div class="group rounded-lg border border-ink-800/30 hover:border-ink-700/50 transition-colors overflow-hidden">
							<div class="grid grid-cols-2 divide-x divide-ink-800/30">
								<div class="p-3 bg-ink-900/20">
									<span class="text-[10px] text-ink-500 font-mono mr-1.5">{i + 1}</span>
									<span class="text-sm text-ink-200 leading-relaxed">{pair.source}</span>
								</div>
								<div class="p-3">
									<span class="text-[10px] text-ink-500 font-mono mr-1.5">{i + 1}</span>
									<span class="text-sm text-accent-300/90 leading-relaxed">{pair.target}</span>
								</div>
							</div>
						</div>
					{/each}
				</div>
			{:else}
				<!-- Side-by-side view (default) -->
				<!-- Source -->
				<div class="flex flex-1 flex-col">
					<div class="border-b border-ink-800/30 px-4 py-2">
						<span class="text-xs font-medium text-ink-400">原文</span>
					</div>
					<textarea
						bind:value={sourceText}
						placeholder="粘贴或输入需要翻译的文本..."
						class="flex-1 resize-none bg-transparent p-4 text-sm text-ink-200 placeholder-ink-600 outline-none leading-relaxed"
					></textarea>
				</div>

				<!-- Target -->
				<div class="flex flex-1 flex-col">
					<div class="border-b border-ink-800/30 px-4 py-2 flex items-center justify-between">
						<span class="text-xs font-medium text-ink-400">译文</span>
						{#if translatedText}
							<button onclick={copyTranslation} class="text-[10px] text-ink-500 hover:text-accent-400 transition-colors">
								复制
							</button>
						{/if}
					</div>
					<div class="flex-1 overflow-y-auto p-4 text-sm text-ink-200 leading-relaxed">
						{#if translating}
							<div class="flex items-center gap-2 text-ink-400">
								<div class="h-3 w-3 animate-spin rounded-full border-2 border-accent-500 border-t-transparent"></div>
								正在翻译...
							</div>
						{:else if translatedText}
							<p class="whitespace-pre-wrap">{translatedText}</p>
							{#if glossaryApplied.length > 0}
								<div class="mt-4 pt-3 border-t border-ink-800/30">
									<p class="text-xs text-ink-500 mb-1">已应用术语:</p>
									<div class="flex flex-wrap gap-1">
										{#each glossaryApplied as term}
											<span class="inline-flex max-w-full items-center gap-1 rounded bg-accent-500/10 px-1.5 py-0.5 text-[10px] text-accent-400">
												<span class="truncate">{term.source_term}</span>
												<span class="text-ink-500">→</span>
												<span class="truncate">{term.target_term}</span>
												<span class="text-ink-600">({term.category})</span>
											</span>
										{/each}
									</div>
								</div>
							{/if}
						{:else}
							<p class="text-ink-600 italic">译文将显示在这里</p>
						{/if}
					</div>
				</div>
			{/if}
		</div>
	</div>

	<!-- Glossary sidebar -->
	{#if glossaryVisible}
		<aside class="w-80 shrink-0 flex flex-col border-l border-ink-800/50 bg-ink-950">
			<div class="border-b border-ink-800/50 px-4 py-3 flex items-center justify-between">
				<h3 class="text-sm font-semibold text-ink-100">术语表</h3>
				<button
					type="button"
						onclick={() => showGlossaryForm = !showGlossaryForm}
						class="inline-flex items-center gap-1 text-xs text-accent-400 transition-colors hover:text-accent-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
					>
						<Plus size={12} />
						添加术语
					</button>
			</div>

			{#if showGlossaryForm}
				<form class="space-y-3 border-b border-ink-800/50 p-4" onsubmit={(e) => { e.preventDefault(); addGlossaryEntry(); }}>
					<div>
						<label for="translate-glossary-term" class="mb-1 block text-xs font-medium text-ink-500">原词</label>
						<input
							id="translate-glossary-term"
							name="translate-glossary-term"
							autocomplete="off"
							bind:value={glossaryForm.term}
							placeholder="例如：萧炎…"
							class="w-full rounded-lg border border-ink-700/60 bg-ink-900/60 px-3 py-2 text-sm text-ink-200 placeholder:text-ink-600 focus:border-accent-500/50 focus:outline-none"
						/>
					</div>
					<div>
						<label for="translate-glossary-definition" class="mb-1 block text-xs font-medium text-ink-500">译名/释义</label>
						<input
							id="translate-glossary-definition"
							name="translate-glossary-definition"
							autocomplete="off"
							bind:value={glossaryForm.definition}
							placeholder="例如：Xiao Yan…"
							class="w-full rounded-lg border border-ink-700/60 bg-ink-900/60 px-3 py-2 text-sm text-ink-200 placeholder:text-ink-600 focus:border-accent-500/50 focus:outline-none"
						/>
					</div>
					<div class="flex items-center justify-end gap-2">
						<button type="button" onclick={() => showGlossaryForm = false} class="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs text-ink-500 transition-colors hover:text-ink-200">
							<X size={12} />
							取消
						</button>
						<button
							type="submit"
							disabled={!glossaryForm.term.trim() || !glossaryForm.definition.trim() || savingGlossary}
							class="inline-flex items-center gap-1 rounded-md bg-accent-500 px-3 py-1.5 text-xs font-medium text-ink-950 transition-colors hover:bg-accent-400 disabled:cursor-not-allowed disabled:opacity-50"
						>
							<Check size={12} />
							{savingGlossary ? '保存中…' : '保存'}
						</button>
					</div>
				</form>
			{/if}

			{#if extractedTerms.length > 0}
				<section class="border-b border-ink-800/50 p-4" aria-label="AI 术语候选">
					<div class="mb-3 flex items-center justify-between gap-3">
						<div>
							<h4 class="text-xs font-semibold text-ink-200">AI 术语候选</h4>
							<p class="mt-0.5 text-[10px] text-ink-500">确认后写入当前翻译术语表</p>
						</div>
						<span class="text-[10px] text-accent-400">{extractedTerms.length} 个</span>
					</div>
					<div class="space-y-2">
						{#each extractedTerms as term}
							<div class="rounded-lg border border-accent-500/20 bg-accent-500/5 p-3">
								<div class="flex items-start justify-between gap-2">
									<div class="min-w-0">
										<div class="flex flex-wrap items-center gap-2">
											<span class="text-sm font-medium text-ink-100">{term.source}</span>
											<span class="text-xs text-accent-300">{term.target}</span>
										</div>
										<div class="mt-1 text-[10px] text-ink-500">{term.category || 'concept'}</div>
										{#if term.context}
											<p class="mt-1 line-clamp-2 text-[10px] text-ink-500">{term.context}</p>
										{/if}
									</div>
									<button
										type="button"
										onclick={() => saveExtractedTerm(term)}
										aria-label={`添加术语 ${term.source}`}
										class="grid h-7 w-7 shrink-0 place-items-center rounded-md text-accent-300 transition-colors hover:bg-accent-500/15 hover:text-accent-200"
									>
										<Check size={13} />
									</button>
								</div>
							</div>
						{/each}
					</div>
				</section>
			{/if}

			{#if glossaryEntries.length === 0}
				<div class="flex-1 flex items-center justify-center p-4">
					<div class="text-center">
						<p class="text-sm text-ink-500">暂无术语</p>
						<p class="mt-1 text-xs text-ink-600">
							添加角色名、专有名词等确保翻译一致性
						</p>
					</div>
				</div>
			{:else}
				<div class="flex-1 overflow-y-auto p-3 space-y-2">
					{#each glossaryEntries as entry}
						<div class="rounded-lg border border-ink-800/50 bg-ink-900/30 p-3">
							<div class="flex items-center justify-between">
								<span class="text-sm font-medium text-ink-200">{entry.source}</span>
								<span class="text-[10px] text-ink-500 bg-ink-800 rounded px-1.5 py-0.5">{entry.category}</span>
							</div>
							<div class="mt-1 text-sm text-accent-400">{entry.target}</div>
							{#if entry.context}
								<div class="mt-1 text-[10px] text-ink-500 italic">{entry.context}</div>
							{/if}
						</div>
					{/each}
				</div>
			{/if}
		</aside>
	{/if}
</div>
