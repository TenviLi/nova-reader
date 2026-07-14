<script lang="ts">
	import { api } from '$services/api';
	import { getErrorMessage } from '$lib/utils';
	import { Check, Plus, X } from 'lucide-svelte';
	import { toast } from 'svelte-sonner';

	let {
		word = '',
		position = { x: 0, y: 0 },
		visible = false,
		bookId = undefined,
		onclose = () => {},
	} = $props<{
		word?: string;
		position?: { x: number; y: number };
		visible?: boolean;
		bookId?: string;
		onclose?: () => void;
	}>();

	let definition = $state<string | null>(null);
	let draftDefinition = $state('');
	let adding = $state(false);
	let saving = $state(false);
	let loading = $state(false);

	$effect(() => {
		if (visible && word) {
			lookupWord(word);
		}
	});

	async function lookupWord(w: string) {
		loading = true;
		definition = null;
		draftDefinition = '';
		adding = false;
		try {
			const data = await api.lookupGlossaryTerm(w, bookId);
			if (data.definition) {
				definition = data.definition;
				return;
			}
			definition = '暂无释义';
		} catch {
			definition = '查询失败';
		} finally {
			loading = false;
		}
	}

	async function saveTerm() {
		const term = word.trim();
		const target = draftDefinition.trim();
		if (!term || !target || saving) return;
		saving = true;
		try {
			await api.createGlossaryEntry({
				term,
				definition: target,
				source_language: 'zh',
				target_language: 'en',
				book_id: bookId ?? null,
			});
			definition = target;
			adding = false;
			toast.success('术语已保存');
		} catch (err) {
			toast.error(getErrorMessage(err) ?? '术语保存失败');
		} finally {
			saving = false;
		}
	}
</script>

{#if visible}
	<div
		class="fixed z-[200] min-w-[200px] max-w-[320px] rounded-xl border border-ink-700 bg-ink-900/95 backdrop-blur-md shadow-2xl p-3 text-sm animate-in fade-in slide-in-from-bottom-2 duration-200"
		style="left: {Math.min(position.x, window.innerWidth - 340)}px; top: {position.y + 10}px;"
		role="tooltip"
	>
		<div class="flex items-center justify-between mb-2">
			<span class="font-semibold text-ink-100 text-base">{word}</span>
			<button
				type="button"
				aria-label="关闭词典释义"
				class="text-ink-500 hover:text-ink-300 p-0.5"
				onclick={onclose}
			>
				<X size={14} />
			</button>
		</div>

		{#if loading}
			<div class="flex items-center gap-2 text-ink-400">
				<div class="w-3 h-3 rounded-full border-2 border-ink-500 border-t-accent-500 animate-spin"></div>
				查询中...
			</div>
		{:else if definition}
			<p class="text-ink-300 leading-relaxed">{definition}</p>
		{/if}

		{#if adding}
			<div class="mt-3 space-y-2 border-t border-ink-800/70 pt-3">
				<label for="dictionary-term-definition" class="text-xs font-medium text-ink-500">译名或释义</label>
				<input
					id="dictionary-term-definition"
					name="dictionary-term-definition"
					autocomplete="off"
					bind:value={draftDefinition}
					placeholder="输入译名…"
					class="w-full rounded-lg border border-ink-700/60 bg-ink-950/60 px-3 py-2 text-xs text-ink-200 placeholder:text-ink-600 focus:border-accent-500/50 focus:outline-none"
				/>
				<button
					type="button"
					onclick={saveTerm}
					disabled={!draftDefinition.trim() || saving}
					class="inline-flex w-full items-center justify-center gap-2 rounded-lg bg-accent-500 px-3 py-1.5 text-xs font-medium text-ink-950 transition-colors hover:bg-accent-400 disabled:cursor-not-allowed disabled:opacity-50"
				>
					<Check size={13} />
					{saving ? '保存中…' : '保存到本书术语表'}
				</button>
			</div>
		{:else}
			<button
				type="button"
				onclick={() => { adding = true; draftDefinition = definition && definition !== '暂无释义' && definition !== '查询失败' ? definition : ''; }}
				class="mt-3 inline-flex items-center gap-1.5 rounded-md border border-ink-700/60 px-2 py-1 text-xs text-ink-400 transition-colors hover:border-accent-500/40 hover:text-accent-300"
			>
				<Plus size={12} />
				添加术语
			</button>
		{/if}
	</div>
{/if}
