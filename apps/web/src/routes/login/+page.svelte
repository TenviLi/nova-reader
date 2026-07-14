<script lang="ts">
import { getErrorMessage } from '$lib/utils';
	import { goto } from '$app/navigation';
	import { api } from '$services/api';
	import { auth } from '$stores/auth.svelte';
	import { toast } from 'svelte-sonner';
	import { BookOpen } from 'lucide-svelte';
	import { onMount } from 'svelte';

	let username = $state('');
	let password = $state('');
	let rememberMe = $state(false);
	let loading = $state(false);
	let error = $state('');
	let checkingSetup = $state(true);
	let needsSetup = $state(false);

	onMount(async () => {
		// If already authenticated, redirect to home
		if (auth.isAuthenticated) {
			goto('/', { replaceState: true });
			return;
		}
		// Check if setup is needed
		try {
			const res = await fetch('/api/health/setup-status');
			const status = await res.json();
			if (status?.needs_setup) {
				needsSetup = true;
				goto('/setup', { replaceState: true });
				return;
			}
		} catch {
			// If endpoint fails, stay on login
		}
		checkingSetup = false;
	});

	async function handleLogin() {
		if (!username || !password) return;

		loading = true;
		error = '';

		try {
			await auth.login(username, password);
			toast.success('登录成功');
			goto('/');
		} catch (e: unknown) {
			error = getErrorMessage(e) ?? '登录失败，请检查用户名和密码';
		} finally {
			loading = false;
		}
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter') handleLogin();
	}
</script>

<svelte:head>
	<title>Nova Reader — 登录</title>
</svelte:head>

{#if checkingSetup}
	<div class="min-h-screen flex items-center justify-center bg-ink-950">
		<div class="animate-pulse text-ink-500 text-sm">正在检查...</div>
	</div>
{:else}
<div class="min-h-screen flex items-center justify-center bg-ink-950 relative overflow-hidden">
	<!-- Decorative background -->
	<div class="absolute inset-0 pointer-events-none">
		<div class="absolute -top-40 -right-40 w-80 h-80 rounded-full bg-amber-500/5 blur-3xl"></div>
		<div class="absolute -bottom-40 -left-40 w-80 h-80 rounded-full bg-violet-500/5 blur-3xl"></div>
	</div>

	<div class="relative w-full max-w-sm p-8">
		<!-- Logo -->
		<div class="text-center mb-8">
			<div class="inline-flex items-center justify-center w-14 h-14 rounded-2xl bg-gradient-to-br from-amber-500 to-amber-700 mb-4">
				<BookOpen class="w-7 h-7 text-ink-950" />
			</div>
			<h1 class="text-xl font-bold text-ink-50">Nova Reader</h1>
			<p class="mt-1 text-sm text-ink-500">登录以继续</p>
		</div>

		<!-- Login Form -->
		<div class="rounded-2xl border border-ink-800 bg-ink-900/90 p-6 space-y-4">
			{#if error}
				<div class="rounded-lg bg-red-500/10 border border-red-500/20 px-4 py-2.5 text-sm text-red-300">
					{error}
				</div>
			{/if}

			<div>
				<label for="login-username" class="block text-sm text-ink-300 mb-1.5">用户名</label>
				<input
					id="login-username"
					type="text"
					bind:value={username}
					placeholder="admin"
					autocomplete="username"
					onkeydown={handleKeydown}
					class="w-full rounded-lg bg-ink-800 border border-ink-700 px-4 py-2.5 text-ink-100 placeholder:text-ink-600 focus:border-amber-500/50 focus:outline-none transition-colors"
				/>
			</div>

			<div>
				<label for="login-password" class="block text-sm text-ink-300 mb-1.5">密码</label>
				<input
					id="login-password"
					type="password"
					bind:value={password}
					placeholder="••••••••"
					autocomplete="current-password"
					onkeydown={handleKeydown}
					class="w-full rounded-lg bg-ink-800 border border-ink-700 px-4 py-2.5 text-ink-100 placeholder:text-ink-600 focus:border-amber-500/50 focus:outline-none transition-colors"
				/>
			</div>

			<div class="flex items-center justify-between">
				<label class="flex items-center gap-2 cursor-pointer">
					<input
						type="checkbox"
						bind:checked={rememberMe}
						class="w-3.5 h-3.5 rounded border-ink-600 bg-ink-800 text-amber-500 focus:ring-amber-500/30 focus:ring-offset-0"
					/>
					<span class="text-xs text-ink-400">记住我</span>
				</label>
			</div>

			<button
				class="w-full mt-2 flex items-center justify-center rounded-lg bg-amber-500 py-2.5 font-medium text-ink-950 hover:bg-amber-400 disabled:opacity-50 transition-colors"
				disabled={!username || !password || loading}
				onclick={handleLogin}
			>
				{#if loading}
					<span class="animate-spin inline-block w-4 h-4 border-2 border-ink-950/30 border-t-ink-950 rounded-full mr-2"></span>
				{/if}
				登录
			</button>
		</div>

		<p class="mt-4 text-center text-xs text-ink-600">
			{#if needsSetup}
				首次使用？前往 <a href="/setup" class="text-amber-400 hover:underline">初始设置</a>
			{/if}
		</p>
	</div>
</div>
{/if}
