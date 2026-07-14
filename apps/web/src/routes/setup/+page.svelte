<script lang="ts">
import { getErrorMessage } from '$lib/utils';
	import { goto } from '$app/navigation';
	import { api } from '$services/api';
	import { toast } from 'svelte-sonner';
	import { BookOpen, FolderOpen, User, Check, ArrowRight } from 'lucide-svelte';

	let step = $state(1);
	let loading = $state(false);

	// Step 1: User creation
	let username = $state('');
	let password = $state('');
	let confirmPassword = $state('');

	// Step 2: Library setup
	let libraryName = $state('');
	let libraryPath = $state('');

	// Step 3: Done
	let setupComplete = $state(false);

	async function createUser() {
		if (password !== confirmPassword) {
			toast.error('密码不一致');
			return;
		}
		if (password.length < 8) {
			toast.error('密码至少需要 8 个字符');
			return;
		}

		loading = true;
		try {
			await api.register({ username, password });
			toast.success('账户创建成功');
			step = 2;
		} catch (e: unknown) {
			toast.error(getErrorMessage(e) ?? '创建失败');
		} finally {
			loading = false;
		}
	}

	async function addLibrary() {
		if (!libraryPath) {
			toast.error('请输入书库路径');
			return;
		}

		loading = true;
		try {
			const library = await api.createLibrary({
				name: libraryName || '默认书库',
				root_path: libraryPath,
				auto_scan: true,
			});
			await api.scanLibrary(library.id);
			toast.success('书库已添加，正在扫描...');
			step = 3;
			setupComplete = true;
		} catch (e: unknown) {
			toast.error(getErrorMessage(e) ?? '添加失败');
		} finally {
			loading = false;
		}
	}

	function finish() {
		goto('/');
	}
</script>

<svelte:head>
	<title>Nova Reader — 初始设置</title>
</svelte:head>

<div class="min-h-screen flex items-center justify-center bg-ink-950">
	<div class="w-full max-w-lg p-8">
		<!-- Logo -->
		<div class="text-center mb-8">
			<div class="inline-flex items-center justify-center w-16 h-16 rounded-2xl bg-gradient-to-br from-amber-500 to-amber-700 mb-4">
				<BookOpen class="w-8 h-8 text-ink-950" />
			</div>
			<h1 class="text-2xl font-bold text-ink-50">欢迎使用 Nova Reader</h1>
			<p class="mt-2 text-sm text-ink-400">你的个人数字文学智库</p>
		</div>

		<!-- Progress Steps -->
		<div class="flex items-center justify-center gap-2 mb-8">
			{#each [1, 2, 3] as s}
				<div class="flex items-center gap-2">
					<div
						class="w-8 h-8 rounded-full flex items-center justify-center text-sm font-medium transition-colors"
						class:bg-amber-500={step >= s}
						class:text-ink-950={step >= s}
						class:bg-ink-800={step < s}
						class:text-ink-400={step < s}
					>
						{#if step > s}
							<Check class="w-4 h-4" />
						{:else}
							{s}
						{/if}
					</div>
					{#if s < 3}
						<div class="w-12 h-0.5 {step > s ? 'bg-amber-500' : 'bg-ink-800'} transition-colors"></div>
					{/if}
				</div>
			{/each}
		</div>

		<!-- Step Content -->
		<div class="rounded-2xl border border-ink-800 bg-ink-900/90 p-6">
			{#if step === 1}
				<!-- Create Account -->
				<div class="space-y-4">
					<div class="flex items-center gap-2 mb-4">
						<User class="w-5 h-5 text-amber-400" />
						<h2 class="text-lg font-semibold text-ink-100">创建管理员账户</h2>
					</div>

					<div>
						<label for="setup-username" class="block text-sm text-ink-300 mb-1.5">用户名</label>
						<input
							id="setup-username"
							type="text"
							bind:value={username}
							placeholder="admin"
							class="w-full rounded-lg bg-ink-800 border border-ink-700 px-4 py-2.5 text-ink-100 placeholder:text-ink-600 focus:border-amber-500/50 focus:outline-none transition-colors"
						/>
					</div>

					<div>
						<label for="setup-password" class="block text-sm text-ink-300 mb-1.5">密码</label>
						<input
							id="setup-password"
							type="password"
							bind:value={password}
							placeholder="••••••••"
							class="w-full rounded-lg bg-ink-800 border border-ink-700 px-4 py-2.5 text-ink-100 placeholder:text-ink-600 focus:border-amber-500/50 focus:outline-none transition-colors"
						/>
					</div>

					<div>
						<label for="setup-confirm-password" class="block text-sm text-ink-300 mb-1.5">确认密码</label>
						<input
							id="setup-confirm-password"
							type="password"
							bind:value={confirmPassword}
							placeholder="••••••••"
							class="w-full rounded-lg bg-ink-800 border border-ink-700 px-4 py-2.5 text-ink-100 placeholder:text-ink-600 focus:border-amber-500/50 focus:outline-none transition-colors"
						/>
					</div>

					<button
						class="w-full mt-4 flex items-center justify-center gap-2 rounded-lg bg-amber-500 py-2.5 font-medium text-ink-950 hover:bg-amber-400 disabled:opacity-50 transition-colors"
						disabled={!username || !password || loading}
						onclick={createUser}
					>
						{#if loading}
							<span class="animate-spin inline-block w-4 h-4 border-2 border-ink-950/30 border-t-ink-950 rounded-full"></span>
						{:else}
							下一步 <ArrowRight class="w-4 h-4" />
						{/if}
					</button>
				</div>
			{:else if step === 2}
				<!-- Add Library -->
				<div class="space-y-4">
					<div class="flex items-center gap-2 mb-4">
						<FolderOpen class="w-5 h-5 text-amber-400" />
						<h2 class="text-lg font-semibold text-ink-100">添加书库目录</h2>
					</div>

					<p class="text-sm text-ink-400 -mt-2">
						指定包含小说文件的本地目录。系统会自动扫描子目录并识别系列。
					</p>

					<div>
						<label for="setup-library-name" class="block text-sm text-ink-300 mb-1.5">书库名称</label>
						<input
							id="setup-library-name"
							type="text"
							bind:value={libraryName}
							placeholder="我的小说"
							class="w-full rounded-lg bg-ink-800 border border-ink-700 px-4 py-2.5 text-ink-100 placeholder:text-ink-600 focus:border-amber-500/50 focus:outline-none transition-colors"
						/>
					</div>

					<div>
						<label for="setup-library-path" class="block text-sm text-ink-300 mb-1.5">目录路径</label>
						<input
							id="setup-library-path"
							type="text"
							bind:value={libraryPath}
							placeholder="/Users/you/Documents/Novels"
							class="w-full rounded-lg bg-ink-800 border border-ink-700 px-4 py-2.5 text-ink-100 placeholder:text-ink-600 focus:border-amber-500/50 focus:outline-none transition-colors font-mono text-sm"
						/>
						<p class="mt-1.5 text-xs text-ink-500">
							目录结构：书库 / 系列文件夹 / 卷册文件（.txt, .epub, .pdf, .md, .docx, .doc）
						</p>
					</div>

					<div class="flex gap-3 mt-4">
						<button
							class="flex-1 flex items-center justify-center gap-2 rounded-lg bg-amber-500 py-2.5 font-medium text-ink-950 hover:bg-amber-400 disabled:opacity-50 transition-colors"
							disabled={!libraryPath || loading}
							onclick={addLibrary}
						>
							{#if loading}
								<span class="animate-spin inline-block w-4 h-4 border-2 border-ink-950/30 border-t-ink-950 rounded-full"></span>
							{:else}
								添加并扫描 <ArrowRight class="w-4 h-4" />
							{/if}
						</button>
						<button
							class="px-4 py-2.5 text-sm text-ink-400 hover:text-ink-200 transition-colors"
							onclick={() => step = 3}
						>
							跳过
						</button>
					</div>
				</div>
			{:else}
				<!-- Done -->
				<div class="text-center py-4">
					<div class="inline-flex items-center justify-center w-16 h-16 rounded-full bg-emerald-500/10 mb-4">
						<Check class="w-8 h-8 text-emerald-400" />
					</div>
					<h2 class="text-xl font-semibold text-ink-100">设置完成！</h2>
					<p class="mt-2 text-sm text-ink-400">
						你的个人小说管理平台已准备就绪。
					</p>

					<button
						class="mt-6 inline-flex items-center gap-2 rounded-lg bg-amber-500 px-6 py-2.5 font-medium text-ink-950 hover:bg-amber-400 transition-colors"
						onclick={finish}
					>
						进入 Nova Reader <ArrowRight class="w-4 h-4" />
					</button>
				</div>
			{/if}
		</div>
	</div>
</div>
