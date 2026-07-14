<script lang="ts">
	import { Check, X, Sparkles, FileText } from 'lucide-svelte';
	import { api } from '$services/api';
	import { slide, fade } from 'svelte/transition';

	interface ProposedNode {
		label: string;
		description: string | null;
		evidence_count: number;
		sample_text: string;
		auto_created: boolean;
		created_id: string | null;
	}

	let {
		parentId,
		parentLabel,
		proposals,
		evidenceChunks,
		onDone,
	}: {
		parentId: string;
		parentLabel: string;
		proposals: ProposedNode[];
		evidenceChunks: number;
		onDone: () => void;
	} = $props();

	function createDecisionMap(items: ProposedNode[]) {
		return Object.fromEntries(items.map((_, i) => [i, null])) as Record<number, 'accepted' | 'rejected' | null>;
	}

	let decisions = $state<Record<number, 'accepted' | 'rejected' | null>>({});
	let submitting = $state(false);
	let expandedSample = $state<number | null>(null);

	$effect(() => {
		decisions = createDecisionMap(proposals);
	});

	let allDecided = $derived(
		proposals.every((_, i) => decisions[i] !== null)
	);
	let acceptedCount = $derived(
		Object.values(decisions).filter(d => d === 'accepted').length
	);
	let rejectedCount = $derived(
		Object.values(decisions).filter(d => d === 'rejected').length
	);

	function acceptAll() {
		for (let i = 0; i < proposals.length; i++) {
			decisions[i] = 'accepted';
		}
	}

	function rejectAll() {
		for (let i = 0; i < proposals.length; i++) {
			decisions[i] = 'rejected';
		}
	}

	async function confirmDecisions() {
		submitting = true;
		try {
			const rejected = proposals
				.filter((_, i) => decisions[i] === 'rejected')
				.map(p => p.created_id)
				.filter((id): id is string => id !== null);

			// Delete rejected nodes that were auto-created
			for (const id of rejected) {
				await api.del(`/ontology/nodes/${id}`);
			}
			onDone();
		} catch (e) {
			console.error('Failed to process decisions:', e);
		} finally {
			submitting = false;
		}
	}
</script>

<div class="rounded-xl border border-green-500/20 bg-green-500/5 p-4 space-y-3" transition:slide>
	<!-- Header -->
	<div class="flex items-center justify-between">
		<div class="flex items-center gap-2">
			<Sparkles class="w-4 h-4 text-green-400" />
			<h4 class="text-sm font-medium text-ink-100">
				进化提案 — <span class="text-green-400">{parentLabel}</span>
			</h4>
		</div>
		<span class="text-[10px] text-ink-500">
			基于 {evidenceChunks} 条证据
		</span>
	</div>

	<!-- Quick actions -->
	<div class="flex items-center gap-2">
		<button
			type="button"
			class="text-[10px] px-2 py-0.5 rounded border border-green-500/30 text-green-400 hover:bg-green-500/10 transition-colors"
			onclick={acceptAll}
		>
			全部接受
		</button>
		<button
			type="button"
			class="text-[10px] px-2 py-0.5 rounded border border-red-500/30 text-red-400 hover:bg-red-500/10 transition-colors"
			onclick={rejectAll}
		>
			全部拒绝
		</button>
		{#if allDecided}
			<span class="text-[10px] text-ink-500 ml-auto">
				{acceptedCount} 接受 · {rejectedCount} 拒绝
			</span>
		{/if}
	</div>

	<!-- Proposals list -->
	<div class="space-y-2">
		{#each proposals as proposal, i}
			{@const decision = decisions[i]}
			<div
				class="rounded-lg border p-3 transition-all duration-200
					{decision === 'accepted' ? 'border-green-500/40 bg-green-500/5' : ''}
					{decision === 'rejected' ? 'border-red-500/30 bg-red-500/5 opacity-60' : ''}
					{decision === null ? 'border-ink-700/50 bg-ink-900/40' : ''}"
				transition:slide
			>
				<div class="flex items-start gap-3">
					<!-- Decision buttons -->
					<div class="flex flex-col gap-1 shrink-0 pt-0.5">
						<button
							type="button"
							class="p-1 rounded transition-colors
								{decision === 'accepted' ? 'bg-green-500 text-white' : 'text-ink-500 hover:text-green-400 hover:bg-green-500/10'}"
							onclick={() => decisions[i] = decisions[i] === 'accepted' ? null : 'accepted'}
							title="接受"
							aria-pressed={decision === 'accepted'}
						>
							<Check class="w-3.5 h-3.5" />
						</button>
						<button
							type="button"
							class="p-1 rounded transition-colors
								{decision === 'rejected' ? 'bg-red-500 text-white' : 'text-ink-500 hover:text-red-400 hover:bg-red-500/10'}"
							onclick={() => decisions[i] = decisions[i] === 'rejected' ? null : 'rejected'}
							title="拒绝"
							aria-pressed={decision === 'rejected'}
						>
							<X class="w-3.5 h-3.5" />
						</button>
					</div>

					<!-- Content -->
					<div class="flex-1 min-w-0">
						<div class="flex items-center gap-2">
							<span class="text-sm font-medium text-ink-100">{proposal.label}</span>
							<span class="text-[10px] px-1.5 py-0.5 rounded-full bg-ink-800 text-ink-400">
								{proposal.evidence_count} 证据
							</span>
						</div>
						{#if proposal.description}
							<p class="text-xs text-ink-400 mt-1">{proposal.description}</p>
						{/if}

						<!-- Sample text toggle -->
						<button
							type="button"
							class="flex items-center gap-1 mt-1.5 text-[10px] text-ink-500 hover:text-ink-300 transition-colors"
							onclick={() => expandedSample = expandedSample === i ? null : i}
							aria-expanded={expandedSample === i}
						>
							<FileText class="w-3 h-3" />
							{expandedSample === i ? '收起样本' : '查看样本'}
						</button>

						{#if expandedSample === i}
							<div class="mt-2 p-2 rounded bg-ink-900/80 border border-ink-800/50" transition:slide>
								<p class="text-xs text-ink-300 leading-relaxed line-clamp-4">{proposal.sample_text}</p>
							</div>
						{/if}
					</div>
				</div>
			</div>
		{/each}
	</div>

	<!-- Confirm button -->
	{#if allDecided}
		<div class="flex justify-end pt-2" transition:fade>
			<button
				class="px-4 py-2 rounded-lg bg-green-600 hover:bg-green-500 text-white text-sm font-medium transition-colors disabled:opacity-50"
				onclick={confirmDecisions}
				disabled={submitting}
			>
				{#if submitting}
					确认中...
				{:else}
					确认决定 ({acceptedCount} 保留, {rejectedCount} 移除)
				{/if}
			</button>
		</div>
	{/if}
</div>
