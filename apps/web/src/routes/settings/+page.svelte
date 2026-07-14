<script lang="ts">
	import { goto } from '$app/navigation';
	import { browser } from '$app/environment';
	import { api } from '$services/api';
	import { auth } from '$stores/auth.svelte';
	import { getErrorMessage } from '$lib/utils';
	import { toast } from 'svelte-sonner';
	import { Bell, BookOpen, Camera, Lock, Palette, Save, Shield, Sparkles, Type, User } from 'lucide-svelte';

	type SettingsSection = 'profile' | 'reading' | 'appearance' | 'ai' | 'notifications' | 'security';

	let activeSection = $state<SettingsSection>('profile');
	let savingProfile = $state(false);
	let savingPreferences = $state(false);

	let profileForm = $state({
		username: auth.user?.username ?? '',
		email: auth.user?.email ?? '',
		currentPassword: '',
		newPassword: '',
		confirmPassword: '',
	});

	let preferences = $state({
		fontSize: browser ? Number(localStorage.getItem('nova_reader_font_size') ?? 18) : 18,
		lineHeight: browser ? Number(localStorage.getItem('nova_reader_line_height') ?? 1.8) : 1.8,
		theme: browser ? localStorage.getItem('nova_theme') ?? 'system' : 'system',
		readerWidth: browser ? localStorage.getItem('nova_reader_width') ?? 'comfortable' : 'comfortable',
		immersiveTranslation: browser ? localStorage.getItem('nova_immersive_translation') === 'true' : false,
		notifyScan: browser ? localStorage.getItem('nova_notify_scan') !== 'false' : true,
		notifyAi: browser ? localStorage.getItem('nova_notify_ai') !== 'false' : true,
		notifyReading: browser ? localStorage.getItem('nova_notify_reading') !== 'false' : true,
	});

	const sections: Array<{ id: SettingsSection; label: string; description: string; icon: typeof User }> = [
		{ id: 'profile', label: '个人资料', description: '账号、头像、邮箱', icon: User },
		{ id: 'reading', label: '阅读', description: '字号、行距、版心', icon: BookOpen },
		{ id: 'appearance', label: '外观', description: '主题、字体资源', icon: Palette },
		{ id: 'ai', label: 'AI', description: '阅读器 AI 行为', icon: Sparkles },
		{ id: 'notifications', label: '通知', description: '活动提醒偏好', icon: Bell },
		{ id: 'security', label: '安全', description: '密码和会话', icon: Shield },
	];

	async function handleAvatarUpload(e: Event) {
		const input = e.target as HTMLInputElement;
		const file = input.files?.[0];
		if (!file) return;
		if (file.size > 5 * 1024 * 1024) {
			toast.error('头像文件不能超过 5MB');
			return;
		}
		try {
			await api.uploadAvatar(file);
			await auth.refresh();
			toast.success('头像已更新');
		} catch (err: unknown) {
			toast.error(getErrorMessage(err) ?? '上传失败');
		}
	}

	async function saveProfile() {
		if (profileForm.newPassword && profileForm.newPassword !== profileForm.confirmPassword) {
			toast.error('两次密码输入不一致');
			return;
		}
		savingProfile = true;
		try {
			if (profileForm.username || profileForm.email) {
				await api.updateProfile({
					username: profileForm.username || undefined,
					email: profileForm.email || undefined,
				});
			}
			if (profileForm.currentPassword && profileForm.newPassword) {
				await api.changePassword({
					current_password: profileForm.currentPassword,
					new_password: profileForm.newPassword,
				});
			}
			await auth.refresh();
			profileForm.currentPassword = '';
			profileForm.newPassword = '';
			profileForm.confirmPassword = '';
			toast.success('资料已保存');
		} catch (err: unknown) {
			toast.error(getErrorMessage(err) ?? '保存失败');
		} finally {
			savingProfile = false;
		}
	}

	function savePreferences() {
		if (!browser) return;
		savingPreferences = true;
		localStorage.setItem('nova_reader_font_size', String(preferences.fontSize));
		localStorage.setItem('nova_reader_line_height', String(preferences.lineHeight));
		localStorage.setItem('nova_theme', preferences.theme);
		localStorage.setItem('nova_reader_width', preferences.readerWidth);
		localStorage.setItem('nova_immersive_translation', String(preferences.immersiveTranslation));
		localStorage.setItem('nova_notify_scan', String(preferences.notifyScan));
		localStorage.setItem('nova_notify_ai', String(preferences.notifyAi));
		localStorage.setItem('nova_notify_reading', String(preferences.notifyReading));
		setTimeout(() => {
			savingPreferences = false;
			toast.success('偏好已保存');
		}, 120);
	}

	async function handleLogout() {
		await api.logout();
		goto('/login');
	}
</script>

<svelte:head>
	<title>Nova Reader — 设置</title>
</svelte:head>

<div class="mx-auto max-w-[1400px] px-4 py-6 sm:px-6 lg:px-8 animate-fade-in">
	<div class="mb-6 flex flex-wrap items-center justify-between gap-4">
		<div>
			<h1 class="text-2xl font-bold text-ink-50">设置</h1>
			<p class="mt-1 text-sm text-ink-400">管理个人资料、阅读体验、AI 行为和通知偏好</p>
		</div>
		<button onclick={savePreferences} disabled={savingPreferences} class="inline-flex items-center gap-2 rounded-lg bg-accent-500 px-4 py-2 text-sm font-medium text-ink-950 transition-colors hover:bg-accent-400 disabled:opacity-50">
			<Save size={15} />
			{savingPreferences ? '保存中...' : '保存偏好'}
		</button>
	</div>

	<div class="grid gap-6 lg:grid-cols-[260px_minmax(0,1fr)]">
		<aside class="h-fit rounded-xl border border-ink-800/50 bg-ink-900/40 p-2">
			{#each sections as section}
				{@const Icon = section.icon}
				<button onclick={() => activeSection = section.id} class="flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left transition-colors {activeSection === section.id ? 'bg-accent-500/10 text-accent-300' : 'text-ink-400 hover:bg-ink-800/60 hover:text-ink-100'}">
					<Icon size={17} />
					<span class="min-w-0">
						<span class="block text-sm font-medium">{section.label}</span>
						<span class="block truncate text-xs opacity-70">{section.description}</span>
					</span>
				</button>
			{/each}
		</aside>

		<main class="space-y-5">
			{#if activeSection === 'profile'}
				<section class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-5">
					<h2 class="flex items-center gap-2 text-base font-semibold text-ink-100"><User size={17} /> 个人资料</h2>
					<div class="mt-5 flex items-center gap-6">
						<div class="relative group">
							<div class="flex h-20 w-20 items-center justify-center overflow-hidden rounded-full border-2 border-ink-700/50 bg-gradient-to-br from-accent-500/30 to-amber-500/20">
								{#if auth.user?.avatar_url}
									<img src={auth.user.avatar_url} alt="头像" class="h-full w-full object-cover" />
								{:else}
									<span class="text-2xl font-bold text-ink-300">{auth.user?.username?.charAt(0).toUpperCase() ?? 'U'}</span>
								{/if}
							</div>
							<label class="absolute inset-0 flex cursor-pointer items-center justify-center rounded-full bg-black/50 opacity-0 transition-opacity group-hover:opacity-100">
								<Camera size={16} class="text-white" />
								<input type="file" accept="image/*" class="hidden" onchange={handleAvatarUpload} />
							</label>
						</div>
						<div>
							<p class="text-sm font-medium text-ink-200">{auth.user?.display_name || (auth.user?.username ?? '用户')}</p>
							<p class="mt-1 text-xs text-ink-500">点击头像更换，支持 JPG/PNG，最大 5MB</p>
						</div>
					</div>

					<div class="mt-6 grid gap-4 md:grid-cols-2">
						<div>
							<label for="settings-username" class="mb-1 block text-xs text-ink-400">用户名</label>
							<input id="settings-username" type="text" bind:value={profileForm.username} class="w-full rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-2 text-sm text-ink-200 outline-none focus:border-accent-500/30" />
						</div>
						<div>
							<label for="settings-email" class="mb-1 block text-xs text-ink-400">邮箱</label>
							<input id="settings-email" type="email" bind:value={profileForm.email} class="w-full rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-2 text-sm text-ink-200 outline-none focus:border-accent-500/30" />
						</div>
					</div>
					<div class="mt-5 flex justify-end">
						<button onclick={saveProfile} disabled={savingProfile} class="rounded-lg bg-accent-500 px-5 py-2 text-sm font-medium text-ink-950 transition-colors hover:bg-accent-400 disabled:opacity-50">{savingProfile ? '保存中...' : '保存资料'}</button>
					</div>
				</section>
			{:else if activeSection === 'reading'}
				<section class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-5">
					<h2 class="flex items-center gap-2 text-base font-semibold text-ink-100"><BookOpen size={17} /> 阅读体验</h2>
					<div class="mt-5 grid gap-5 md:grid-cols-2">
						<label class="space-y-2">
							<span class="text-xs text-ink-400">正文字号：{preferences.fontSize}px</span>
							<input type="range" min="14" max="26" bind:value={preferences.fontSize} class="w-full accent-amber-500" />
						</label>
						<label class="space-y-2">
							<span class="text-xs text-ink-400">行距：{preferences.lineHeight.toFixed(1)}</span>
							<input type="range" min="1.4" max="2.4" step="0.1" bind:value={preferences.lineHeight} class="w-full accent-amber-500" />
						</label>
						<div>
							<label for="reader-width" class="mb-1 block text-xs text-ink-400">阅读版心</label>
							<select id="reader-width" bind:value={preferences.readerWidth} class="w-full rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-2 text-sm text-ink-200 outline-none focus:border-accent-500/30">
								<option value="narrow">窄版</option>
								<option value="comfortable">舒适</option>
								<option value="wide">宽版</option>
							</select>
						</div>
					</div>
				</section>
			{:else if activeSection === 'appearance'}
				<section class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-5">
					<h2 class="flex items-center gap-2 text-base font-semibold text-ink-100"><Palette size={17} /> 外观</h2>
					<div class="mt-5 grid gap-4 md:grid-cols-2">
						<div>
							<label for="theme-mode" class="mb-1 block text-xs text-ink-400">主题</label>
							<select id="theme-mode" bind:value={preferences.theme} class="w-full rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-2 text-sm text-ink-200 outline-none focus:border-accent-500/30">
								<option value="system">跟随系统</option>
								<option value="dark">深色</option>
								<option value="light">浅色</option>
							</select>
						</div>
						<a href="/settings/fonts" class="flex items-center justify-between rounded-lg border border-ink-800/60 bg-ink-950/30 p-3 text-sm text-ink-300 transition-colors hover:border-accent-500/30 hover:text-accent-300">
							<span class="inline-flex items-center gap-2"><Type size={15} /> 字体管理</span>
							<span class="text-xs text-ink-500">打开</span>
						</a>
					</div>
				</section>
			{:else if activeSection === 'ai'}
				<section class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-5">
					<h2 class="flex items-center gap-2 text-base font-semibold text-ink-100"><Sparkles size={17} /> AI 阅读行为</h2>
					<label class="mt-5 flex items-center justify-between rounded-lg border border-ink-800/60 bg-ink-950/30 p-3">
						<span>
							<span class="block text-sm text-ink-200">默认启用沉浸式翻译</span>
							<span class="text-xs text-ink-500">阅读器进入章节时记住双语/译文偏好</span>
						</span>
						<input type="checkbox" bind:checked={preferences.immersiveTranslation} class="h-4 w-4 rounded border-ink-600 bg-ink-800 text-accent-500" />
					</label>
				</section>
			{:else if activeSection === 'notifications'}
				<section class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-5">
					<h2 class="flex items-center gap-2 text-base font-semibold text-ink-100"><Bell size={17} /> 通知偏好</h2>
					<div class="mt-5 space-y-3">
						<label class="flex items-center justify-between rounded-lg border border-ink-800/60 bg-ink-950/30 p-3 text-sm text-ink-200">书库扫描完成 <input type="checkbox" bind:checked={preferences.notifyScan} class="h-4 w-4" /></label>
						<label class="flex items-center justify-between rounded-lg border border-ink-800/60 bg-ink-950/30 p-3 text-sm text-ink-200">AI 任务完成/失败 <input type="checkbox" bind:checked={preferences.notifyAi} class="h-4 w-4" /></label>
						<label class="flex items-center justify-between rounded-lg border border-ink-800/60 bg-ink-950/30 p-3 text-sm text-ink-200">阅读进度与批注活动 <input type="checkbox" bind:checked={preferences.notifyReading} class="h-4 w-4" /></label>
					</div>
				</section>
			{:else if activeSection === 'security'}
				<section class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-5">
					<h2 class="flex items-center gap-2 text-base font-semibold text-ink-100"><Lock size={17} /> 安全</h2>
					<div class="mt-5 grid gap-4 md:grid-cols-3">
						<div>
							<label for="current-password" class="mb-1 block text-xs text-ink-400">当前密码</label>
							<input id="current-password" type="password" bind:value={profileForm.currentPassword} class="w-full rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-2 text-sm text-ink-200 outline-none focus:border-accent-500/30" />
						</div>
						<div>
							<label for="new-password" class="mb-1 block text-xs text-ink-400">新密码</label>
							<input id="new-password" type="password" bind:value={profileForm.newPassword} class="w-full rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-2 text-sm text-ink-200 outline-none focus:border-accent-500/30" />
						</div>
						<div>
							<label for="confirm-password" class="mb-1 block text-xs text-ink-400">确认新密码</label>
							<input id="confirm-password" type="password" bind:value={profileForm.confirmPassword} class="w-full rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-2 text-sm text-ink-200 outline-none focus:border-accent-500/30" />
						</div>
					</div>
					<div class="mt-5 flex items-center justify-between border-t border-ink-800/50 pt-4">
						<button onclick={handleLogout} class="text-sm text-red-400 transition-colors hover:text-red-300">退出登录</button>
						<button onclick={saveProfile} disabled={savingProfile} class="rounded-lg bg-accent-500 px-5 py-2 text-sm font-medium text-ink-950 transition-colors hover:bg-accent-400 disabled:opacity-50">{savingProfile ? '保存中...' : '保存账号设置'}</button>
					</div>
				</section>
			{/if}
		</main>
	</div>
</div>
