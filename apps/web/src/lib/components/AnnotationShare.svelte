<script lang="ts">
	import { api } from '$services/api';
	import { Copy, ExternalLink, Check } from 'lucide-svelte';
	import { toast } from 'svelte-sonner';

	let {
		annotationId,
		bookTitle = '',
		text = '',
		note = '',
		showPreview = true,
	} = $props<{
		annotationId: string;
		bookTitle?: string;
		text?: string;
		note?: string;
		showPreview?: boolean;
	}>();

	let shareUrl = $state('');
	let sharing = $state(false);
	let copied = $state(false);

	async function generateShareLink() {
		sharing = true;
		try {
			const data = await api.shareAnnotation(annotationId);
			shareUrl = `${window.location.origin}/share/annotation/${data.token}`;
			toast.success('分享链接已生成');
		} catch {
			toast.error('生成分享链接失败');
		} finally {
			sharing = false;
		}
	}

	async function copyLink() {
		await navigator.clipboard.writeText(shareUrl);
		copied = true;
		setTimeout(() => copied = false, 2000);
	}
</script>

<div class="space-y-4">
	<!-- Preview card -->
	{#if showPreview}
		<div class="rounded-lg border border-ink-700 bg-ink-800/50 p-4">
			<p class="text-xs text-ink-500 mb-1">《{bookTitle}》</p>
			<blockquote class="border-l-2 border-accent-500 pl-3 text-ink-200 italic text-sm">
				"{text}"
			</blockquote>
			{#if note}
				<p class="mt-2 text-sm text-ink-300">💭 {note}</p>
			{/if}
		</div>
	{/if}

	<!-- Actions -->
	{#if !shareUrl}
		<button
			class="w-full flex items-center justify-center gap-2 rounded-lg bg-accent-600 hover:bg-accent-500 text-white py-2.5 px-4 text-sm font-medium transition-colors disabled:opacity-50"
			onclick={generateShareLink}
			disabled={sharing}
		>
			<ExternalLink size={16} />
			{sharing ? '生成中...' : '生成公开分享链接'}
		</button>
	{:else}
		<div class="flex items-center gap-2">
			<input
				type="text"
				value={shareUrl}
				readonly
				class="flex-1 rounded-lg border border-ink-700 bg-ink-800 px-3 py-2 text-sm text-ink-200 font-mono"
			/>
			<button
				class="flex items-center gap-1.5 rounded-lg bg-ink-700 hover:bg-ink-600 px-3 py-2 text-sm text-ink-200 transition-colors"
				onclick={copyLink}
				aria-label="复制分享链接"
			>
				{#if copied}
					<Check size={14} class="text-green-400" />
				{:else}
					<Copy size={14} />
				{/if}
			</button>
		</div>
	{/if}
</div>
