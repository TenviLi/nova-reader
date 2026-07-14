<script lang="ts">
	import { AlertTriangle, ShieldCheck } from 'lucide-svelte';
</script>

<svelte:head>
	<title>OPDS 暂未启用 — Nova Reader Admin</title>
</svelte:head>

<div class="mx-auto max-w-3xl px-4 py-6 sm:px-6 lg:px-8">
	<div class="rounded-lg border border-amber-500/20 bg-amber-500/10 p-5">
		<div class="flex items-start gap-3">
			<div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-amber-500/10 text-amber-300">
				<AlertTriangle size={20} />
			</div>
			<div>
				<h1 class="text-lg font-semibold text-ink-100">OPDS 暂未启用</h1>
				<p class="mt-2 text-sm leading-6 text-ink-400">
					当前后端没有挂载 OPDS 路由。直接展示 /opds 地址会造成不可用入口；在完成 OPDS 鉴权和书库权限过滤后，这里再开放连接地址。
				</p>
			</div>
		</div>
	</div>

	<div class="mt-4 rounded-lg border border-ink-800/50 bg-ink-900/30 p-5">
		<div class="flex items-start gap-3">
			<div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-ink-800/60 text-ink-300">
				<ShieldCheck size={20} />
			</div>
			<div>
				<h2 class="text-sm font-medium text-ink-200">启用前置条件</h2>
				<ul class="mt-2 space-y-1 text-sm text-ink-500">
					<li>OPDS feed 必须按登录用户过滤可见书库和书籍。</li>
					<li>下载、封面和搜索链接必须与已注册后端路由一致。</li>
					<li>Admin 页面只在服务真实可用后显示连接 URL 和二维码。</li>
				</ul>
			</div>
		</div>
	</div>
</div>
