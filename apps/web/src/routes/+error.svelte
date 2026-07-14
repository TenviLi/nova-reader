<script lang="ts">
	import { page } from '$app/stores';

	let { error } = $props<{ error: { message: string; status?: number } }>();
</script>

<div class="flex h-[calc(100vh-8rem)] items-center justify-center">
	<div class="text-center max-w-md">
		<div class="mb-6">
			<span class="text-7xl font-bold text-ink-700">{error.status ?? '💥'}</span>
		</div>
		<h1 class="text-xl font-semibold text-ink-100 mb-2">
			{#if error.status === 404}
				页面未找到
			{:else if error.status === 403}
				无访问权限
			{:else if error.status === 500}
				服务器开小差了
			{:else}
				出了点问题
			{/if}
		</h1>
		<p class="text-sm text-ink-400 mb-6">{error.message}</p>
		<div class="flex gap-3 justify-center">
			<button
				onclick={() => window.location.reload()}
				class="px-4 py-2 bg-accent-500 text-ink-950 rounded-lg hover:bg-accent-400 transition-colors text-sm font-medium"
			>
				重试
			</button>
			<a
				href="/"
				class="px-4 py-2 text-ink-300 border border-ink-700/50 hover:bg-ink-800/50 rounded-lg transition-colors text-sm"
			>
				返回首页
			</a>
		</div>
	</div>
</div>
