<script lang="ts">
	import { page } from '$app/stores';
	import { api } from '$services/api';
	import { onMount } from 'svelte';
	import { BookOpen, Share2, Copy, ExternalLink } from 'lucide-svelte';
	import { toast } from 'svelte-sonner';

	interface SharedAnnotation {
		id?: string;
		book_id?: string;
		chapter_index?: number;
		start_offset?: number;
		end_offset?: number;
		selected_text?: string;
		text?: string;
		note?: string | null;
		color?: string;
		book_title?: string;
		chapter_title?: string;
		content?: string;
	}

	let annotationId = $derived($page.params.id!);
	let loading = $state(true);
	let annotation = $state<SharedAnnotation | null>(null);
	let notFound = $state(false);

	onMount(async () => {
		try {
			annotation = await api.getSharedAnnotation(annotationId);
		} catch {
			notFound = true;
		} finally {
			loading = false;
		}
	});

	function copyLink() {
		navigator.clipboard.writeText(window.location.href);
		toast.success('链接已复制');
	}

	function shareToTwitter() {
		const content = annotation?.content ?? annotation?.selected_text ?? annotation?.text ?? '';
		const text = content ? `"${content.slice(0, 100)}..." — ${annotation?.book_title ?? ''}` : '';
		const url = encodeURIComponent(window.location.href);
		window.open(`https://twitter.com/intent/tweet?text=${encodeURIComponent(text)}&url=${url}`, '_blank');
	}

	let ogTitle = $derived(annotation?.book_title ? `批注 — ${annotation.book_title}` : 'Nova Reader 批注分享');
	let quoteText = $derived(annotation?.content ?? annotation?.selected_text ?? annotation?.text ?? '');
	let ogDescription = $derived(quoteText.slice(0, 200));
</script>

<svelte:head>
	<title>{ogTitle}</title>
	<meta property="og:title" content={ogTitle} />
	<meta property="og:description" content={ogDescription} />
	<meta property="og:type" content="article" />
	<meta name="twitter:card" content="summary" />
	<meta name="twitter:title" content={ogTitle} />
	<meta name="twitter:description" content={ogDescription} />
</svelte:head>

<div class="min-h-screen flex items-center justify-center px-4 py-12">
	{#if loading}
		<div class="animate-pulse w-full max-w-lg">
			<div class="h-48 rounded-2xl bg-ink-800/50"></div>
		</div>
	{:else if notFound}
		<div class="text-center">
			<p class="text-ink-400 text-lg">批注不存在或未公开分享</p>
			<a href="/" class="mt-4 inline-block text-sm text-accent-400 hover:text-accent-300">返回首页</a>
		</div>
	{:else if annotation}
		<!-- Annotation Share Card -->
		<div class="w-full max-w-lg">
			<div class="relative overflow-hidden rounded-2xl border border-ink-700/50 bg-gradient-to-b from-ink-900 to-ink-950 shadow-2xl">
				<!-- Decorative accent line -->
				<div class="absolute top-0 left-0 right-0 h-1 bg-gradient-to-r from-accent-500 via-amber-400 to-accent-600"></div>

				<div class="p-8">
					<!-- Quote mark -->
					<div class="mb-4 text-4xl leading-none text-accent-500/40 font-serif">"</div>

					<!-- Highlighted text -->
					{#if quoteText}
						<blockquote class="text-lg text-ink-100 leading-relaxed font-serif italic">
							{quoteText}
						</blockquote>
					{/if}

					<!-- Annotation note -->
					{#if annotation.note}
						<div class="mt-4 pl-4 border-l-2 border-accent-500/30">
							<p class="text-sm text-ink-300">{annotation.note}</p>
						</div>
					{/if}

					<!-- Source info -->
					<div class="mt-6 flex items-center gap-3 pt-4 border-t border-ink-800/50">
						<div class="flex h-10 w-7 items-center justify-center rounded bg-ink-800 shrink-0">
							<BookOpen size={14} class="text-ink-500" />
						</div>
						<div class="flex-1 min-w-0">
							<p class="text-sm font-medium text-ink-200 truncate">{annotation.book_title ?? '未知书名'}</p>
							{#if annotation.chapter_title}
								<p class="text-xs text-ink-500 truncate">{annotation.chapter_title}</p>
							{/if}
						</div>
					</div>

					<!-- Color indicator -->
					{#if annotation.color}
						<div class="mt-4 flex items-center gap-2">
							<div class="h-3 w-3 rounded-full" style="background: {annotation.color}"></div>
							<span class="text-[10px] text-ink-500 uppercase tracking-wider">高亮批注</span>
						</div>
					{/if}
				</div>

				<!-- Share actions -->
				<div class="flex items-center justify-between border-t border-ink-800/50 px-8 py-4 bg-ink-950/50">
					<span class="text-xs text-ink-500">Nova Reader</span>
					<div class="flex items-center gap-2">
						<button
							onclick={copyLink}
							class="rounded-lg p-2 text-ink-400 hover:text-accent-400 hover:bg-accent-500/10 transition-colors"
							title="复制链接"
						>
							<Copy size={16} strokeWidth={1.5} />
						</button>
						<button
							onclick={shareToTwitter}
							class="rounded-lg p-2 text-ink-400 hover:text-accent-400 hover:bg-accent-500/10 transition-colors"
							title="分享到 X"
						>
							<ExternalLink size={16} strokeWidth={1.5} />
						</button>
					</div>
				</div>
			</div>

			<!-- Back link -->
			<div class="mt-6 text-center">
				<a href="/" class="text-sm text-ink-500 hover:text-accent-400 transition-colors">
					在 Nova Reader 中查看更多 →
				</a>
			</div>
		</div>
	{/if}
</div>
