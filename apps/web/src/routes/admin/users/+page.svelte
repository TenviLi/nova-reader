<script lang="ts">
	import { createQuery, createMutation, useQueryClient } from '@tanstack/svelte-query';
	import { api } from '$services/api';
	import type { PermissionTemplate } from '$types/models';
	import { toast } from 'svelte-sonner';
	import { Users, Trash2, Shield, BookOpen, Clock, UserCog, UsersRound, Plus, X, Pencil, Check, KeyRound } from 'lucide-svelte';

	const queryClient = useQueryClient();

	const users = createQuery(() => ({
		queryKey: ['admin', 'users'],
		queryFn: () => api.getUsers(),
	}));

	const groups = createQuery(() => ({
		queryKey: ['admin', 'groups'],
		queryFn: () => api.getGroups(),
	}));

	const permissionTemplates = createQuery(() => ({
		queryKey: ['admin', 'permission-templates'],
		queryFn: () => api.getPermissionTemplates(),
	}));

	let confirmDeleteId = $state<string | null>(null);

	// ─── Batch selection ───────────────────────────────────────
	let selectedIds = $state<Set<string>>(new Set());
	let batchRole = $state<'admin' | 'reader' | 'guest'>('reader');

	function toggleSelect(id: string) {
		const next = new Set(selectedIds);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		selectedIds = next;
	}
	function toggleSelectAll() {
		const all = users.data ?? [];
		if (selectedIds.size === all.length) {
			selectedIds = new Set();
		} else {
			selectedIds = new Set(all.map((u) => u.id));
		}
	}

	const deleteMutation = createMutation(() => ({
		mutationFn: (userId: string) => api.deleteUser(userId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin', 'users'] });
			toast.success('用户已删除');
			confirmDeleteId = null;
		},
		onError: () => toast.error('删除失败'),
	}));

	const updateMutation = createMutation(() => ({
		mutationFn: ({ userId, role }: { userId: string; role: 'admin' | 'reader' | 'guest' }) => api.updateUser(userId, { role }),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin', 'users'] });
			toast.success('用户权限已更新');
		},
		onError: (err) => toast.error(err instanceof Error ? err.message : '更新失败'),
	}));

	const batchRoleMutation = createMutation(() => ({
		mutationFn: ({ userIds, role }: { userIds: string[]; role: 'admin' | 'reader' | 'guest' }) => api.batchUpdateUserRole(userIds, role),
		onSuccess: (res) => {
			queryClient.invalidateQueries({ queryKey: ['admin', 'users'] });
			selectedIds = new Set();
			toast.success(`已更新 ${res.updated} 位用户的角色`);
		},
		onError: (err) => toast.error(err instanceof Error ? err.message : '批量更新失败'),
	}));

	// ─── Groups ────────────────────────────────────────────────
	let showGroupForm = $state(false);
	let newGroupName = $state('');
	let newGroupDesc = $state('');
	let newGroupColor = $state('slate');
	let editingGroupId = $state<string | null>(null);
	let groupDraftName = $state('');
	let groupDraftDesc = $state('');
	let groupDraftColor = $state('slate');
	let editingMembersFor = $state<string | null>(null);
	let memberDraft = $state<Set<string>>(new Set());
	const groupColors = ['slate', 'amber', 'emerald', 'sky', 'violet', 'rose'];
	const groupColorStyles: Record<string, string> = {
		slate: '#64748b',
		amber: '#f59e0b',
		emerald: '#10b981',
		sky: '#0ea5e9',
		violet: '#8b5cf6',
		rose: '#f43f5e',
	};

	const createGroupMutation = createMutation(() => ({
		mutationFn: () => api.createGroup({ name: newGroupName.trim(), description: newGroupDesc.trim(), color: newGroupColor }),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin', 'groups'] });
			toast.success('用户组已创建');
			newGroupName = '';
			newGroupDesc = '';
			newGroupColor = 'slate';
			showGroupForm = false;
		},
		onError: (err) => toast.error(err instanceof Error ? err.message : '创建失败'),
	}));

	const updateGroupMutation = createMutation(() => ({
		mutationFn: (id: string) => api.updateGroup(id, {
			name: groupDraftName.trim(),
			description: groupDraftDesc.trim(),
			color: groupDraftColor,
		}),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin', 'groups'] });
			toast.success('用户组已更新');
			editingGroupId = null;
		},
		onError: (err) => toast.error(err instanceof Error ? err.message : '更新失败'),
	}));

	const deleteGroupMutation = createMutation(() => ({
		mutationFn: (id: string) => api.deleteGroup(id),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin', 'groups'] });
			toast.success('用户组已删除');
		},
		onError: () => toast.error('删除失败'),
	}));

	const setMembersMutation = createMutation(() => ({
		mutationFn: ({ id, userIds }: { id: string; userIds: string[] }) => api.setGroupMembers(id, userIds),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin', 'groups'] });
			toast.success('成员已更新');
			editingMembersFor = null;
		},
		onError: () => toast.error('保存失败'),
	}));

	function openMemberEditor(groupId: string, memberIds: string[]) {
		editingMembersFor = groupId;
		memberDraft = new Set(memberIds);
	}

	function openGroupEditor(group: { id: string; name: string; description: string; color: string }) {
		editingGroupId = group.id;
		groupDraftName = group.name;
		groupDraftDesc = group.description ?? '';
		groupDraftColor = group.color || 'slate';
	}

	function toggleMember(userId: string) {
		const next = new Set(memberDraft);
		if (next.has(userId)) next.delete(userId);
		else next.add(userId);
		memberDraft = next;
	}

	// ─── Permission Templates ──────────────────────────────────────
	let showTemplateForm = $state(false);
	let templateName = $state('');
	let templateDesc = $state('');
	let templateRead = $state(true);
	let templateWrite = $state(false);
	let templateManage = $state(false);
	let editingTemplateId = $state<string | null>(null);
	let editTemplateName = $state('');
	let editTemplateDesc = $state('');
	let editTemplateRead = $state(true);
	let editTemplateWrite = $state(false);
	let editTemplateManage = $state(false);

	function normalizeTemplate(read: boolean, write: boolean, manage: boolean) {
		return {
			can_read: read || write || manage,
			can_write: write || manage,
			can_manage: manage,
		};
	}

	function resetTemplateForm() {
		templateName = '';
		templateDesc = '';
		templateRead = true;
		templateWrite = false;
		templateManage = false;
		showTemplateForm = false;
	}

	function openTemplateEditor(template: PermissionTemplate) {
		editingTemplateId = template.id;
		editTemplateName = template.name;
		editTemplateDesc = template.description ?? '';
		editTemplateRead = template.can_read;
		editTemplateWrite = template.can_write;
		editTemplateManage = template.can_manage;
	}

	const createTemplateMutation = createMutation(() => ({
		mutationFn: () => api.createPermissionTemplate({
			name: templateName.trim(),
			description: templateDesc.trim() || null,
			...normalizeTemplate(templateRead, templateWrite, templateManage),
		}),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin', 'permission-templates'] });
			toast.success('权限模板已创建');
			resetTemplateForm();
		},
		onError: (err) => toast.error(err instanceof Error ? err.message : '创建失败'),
	}));

	const updateTemplateMutation = createMutation(() => ({
		mutationFn: (id: string) => api.updatePermissionTemplate(id, {
			name: editTemplateName.trim(),
			description: editTemplateDesc.trim() || null,
			...normalizeTemplate(editTemplateRead, editTemplateWrite, editTemplateManage),
		}),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin', 'permission-templates'] });
			toast.success('权限模板已更新');
			editingTemplateId = null;
		},
		onError: (err) => toast.error(err instanceof Error ? err.message : '更新失败'),
	}));

	const deleteTemplateMutation = createMutation(() => ({
		mutationFn: (id: string) => api.deletePermissionTemplate(id),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin', 'permission-templates'] });
			toast.success('权限模板已删除');
		},
		onError: (err) => toast.error(err instanceof Error ? err.message : '删除失败'),
	}));

	const roleLabels: Record<string, string> = {
		admin: '管理员',
		reader: '读者',
		guest: '访客',
	};

	function userName(id: string): string {
		const u = (users.data ?? []).find((x) => x.id === id);
		return u ? (u.display_name ?? u.username) : id.slice(0, 8);
	}

	function timeAgo(dateStr: string | null): string {
		if (!dateStr) return '从未';
		const diff = Date.now() - new Date(dateStr).getTime();
		const mins = Math.floor(diff / 60_000);
		if (mins < 1) return '刚刚';
		if (mins < 60) return `${mins}分钟前`;
		const hours = Math.floor(mins / 60);
		if (hours < 24) return `${hours}小时前`;
		const days = Math.floor(hours / 24);
		if (days < 30) return `${days}天前`;
		return `${Math.floor(days / 30)}月前`;
	}
</script>

<svelte:head>
	<title>用户管理 — Nova Reader Admin</title>
</svelte:head>

<div class="mx-auto max-w-[1600px] px-4 py-6 sm:px-6 lg:px-8 space-y-6 animate-fade-in">
	<div>
		<h1 class="text-2xl font-bold text-ink-50">用户管理</h1>
		<p class="mt-1 text-sm text-ink-400">查看用户、调整角色并管理访问权限</p>
	</div>

	{#if users.isLoading}
		<div class="space-y-3">
			{#each Array(4) as _}
				<div class="h-16 rounded-xl bg-ink-900/50 animate-pulse"></div>
			{/each}
		</div>
	{:else if users.data && users.data.length > 0}
		<div class="grid gap-4 md:grid-cols-3">
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<p class="text-xs text-ink-500">用户总数</p>
				<p class="mt-1 text-2xl font-bold text-ink-100">{users.data.length}</p>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<p class="text-xs text-ink-500">管理员</p>
				<p class="mt-1 text-2xl font-bold text-accent-300">{users.data.filter(user => user.role === 'admin').length}</p>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<p class="text-xs text-ink-500">读者/访客</p>
				<p class="mt-1 text-2xl font-bold text-ink-100">{users.data.filter(user => user.role !== 'admin').length}</p>
			</div>
		</div>

		<div class="rounded-xl border border-ink-800/50 overflow-hidden">
			{#if selectedIds.size > 0}
				<div class="flex flex-wrap items-center gap-3 bg-accent-500/10 px-4 py-2.5 text-sm border-b border-accent-500/20">
					<span class="text-accent-200">已选择 <strong>{selectedIds.size}</strong> 位用户</span>
					<div class="ml-auto flex items-center gap-2">
						<label class="text-xs text-ink-400" for="batch-role">设为</label>
						<select
							id="batch-role"
							bind:value={batchRole}
							class="rounded-lg border border-ink-800/70 bg-ink-950/40 px-2 py-1 text-xs text-ink-200 outline-none"
						>
							{#each Object.entries(roleLabels) as [value, label]}
								<option value={value}>{label}</option>
							{/each}
						</select>
						<button
							onclick={() => batchRoleMutation.mutate({ userIds: [...selectedIds], role: batchRole })}
							disabled={batchRoleMutation.isPending}
							class="rounded-lg bg-accent-500/20 px-3 py-1 text-xs font-medium text-accent-200 hover:bg-accent-500/30 disabled:opacity-50"
						>应用</button>
						<button
							onclick={() => selectedIds = new Set()}
							class="rounded-lg px-2 py-1 text-xs text-ink-400 hover:text-ink-200"
						>清除</button>
					</div>
				</div>
			{/if}
			<table class="w-full text-sm">
				<thead>
					<tr class="bg-ink-900/80 text-ink-400 text-left text-xs">
						<th class="px-4 py-3 w-10">
							<input
								type="checkbox"
								checked={selectedIds.size > 0 && selectedIds.size === (users.data?.length ?? 0)}
								onchange={toggleSelectAll}
								class="accent-accent-500"
								aria-label="全选"
							/>
						</th>
						<th class="px-4 py-3">用户</th>
						<th class="px-4 py-3">角色</th>
						<th class="px-4 py-3">书籍</th>
						<th class="px-4 py-3">阅读时长</th>
						<th class="px-4 py-3">上次登录</th>
						<th class="px-4 py-3">注册时间</th>
						<th class="px-4 py-3 w-20"></th>
					</tr>
				</thead>
				<tbody class="divide-y divide-ink-800/30">
					{#each users.data as user}
						<tr class="hover:bg-ink-900/40 transition-colors {selectedIds.has(user.id) ? 'bg-accent-500/5' : ''}">
							<td class="px-4 py-3">
								<input
									type="checkbox"
									checked={selectedIds.has(user.id)}
									onchange={() => toggleSelect(user.id)}
									class="accent-accent-500"
									aria-label="选择 {user.username}"
								/>
							</td>
							<td class="px-4 py-3">
								<div class="flex items-center gap-3">
									<div class="flex h-8 w-8 items-center justify-center rounded-full bg-accent-500/10 text-accent-400">
										<Shield class="w-3.5 h-3.5" />
									</div>
									<div>
										<div class="font-medium text-ink-200">{user.display_name ?? user.username}</div>
										<div class="text-xs text-ink-500">@{user.username}</div>
									</div>
								</div>
							</td>
							<td class="px-4 py-3">
								<label class="sr-only" for="role-{user.id}">角色</label>
								<div class="inline-flex items-center gap-2 rounded-lg border border-ink-800/70 bg-ink-950/40 px-2 py-1">
									<UserCog class="h-3.5 w-3.5 text-ink-500" />
									<select
										id="role-{user.id}"
										value={user.role ?? 'reader'}
										disabled={updateMutation.isPending}
										onchange={(event) => updateMutation.mutate({ userId: user.id, role: (event.currentTarget as HTMLSelectElement).value as 'admin' | 'reader' | 'guest' })}
										class="bg-transparent text-xs text-ink-300 outline-none disabled:opacity-50"
									>
										{#each Object.entries(roleLabels) as [value, label]}
											<option value={value}>{label}</option>
										{/each}
									</select>
								</div>
							</td>
							<td class="px-4 py-3">
								<span class="flex items-center gap-1.5 text-ink-300">
									<BookOpen class="w-3.5 h-3.5 text-ink-500" />
									{user.books_count}
								</span>
							</td>
							<td class="px-4 py-3">
								<span class="flex items-center gap-1.5 text-ink-300">
									<Clock class="w-3.5 h-3.5 text-ink-500" />
									{(user.reading_time_hours ?? 0).toFixed(1)}h
								</span>
							</td>
							<td class="px-4 py-3 text-ink-400 text-xs">{timeAgo(user.last_login_at)}</td>
							<td class="px-4 py-3 text-ink-500 text-xs">{new Date(user.created_at).toLocaleDateString('zh-CN')}</td>
							<td class="px-4 py-3">
								{#if confirmDeleteId === user.id}
									<div class="flex items-center gap-1">
										<button
											onclick={() => deleteMutation.mutate(user.id)}
											class="px-2 py-1 text-[10px] rounded bg-red-500/20 text-red-400 hover:bg-red-500/30"
										>确认</button>
										<button
											onclick={() => confirmDeleteId = null}
											class="px-2 py-1 text-[10px] rounded text-ink-500 hover:text-ink-300"
										>取消</button>
									</div>
								{:else}
									<button
										onclick={() => confirmDeleteId = user.id}
										class="p-1.5 rounded text-ink-600 hover:text-red-400 hover:bg-red-500/10 transition-colors"
										title="删除用户"
									>
										<Trash2 class="w-3.5 h-3.5" />
									</button>
								{/if}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
			</div>

			<!-- Permission Templates -->
			<div class="space-y-3">
				<div class="flex flex-wrap items-center justify-between gap-3">
					<h2 class="flex min-w-0 items-center gap-2 text-lg font-semibold text-ink-100">
						<KeyRound class="h-4.5 w-4.5 text-accent-400" />
						权限模板
						<span class="text-xs font-normal text-ink-500">维护书库授权时可复用的读写策略</span>
					</h2>
					<button
						onclick={() => showTemplateForm = !showTemplateForm}
						class="inline-flex items-center gap-1.5 rounded-lg bg-accent-500/15 px-3 py-1.5 text-xs font-medium text-accent-200 hover:bg-accent-500/25"
					>
						<Plus class="h-3.5 w-3.5" /> 新建模板
					</button>
				</div>

				{#if showTemplateForm}
					<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
						<div class="grid gap-3 md:grid-cols-[1fr_1.5fr_auto]">
							<div>
								<label class="mb-1 block text-xs text-ink-500" for="template-name">模板名称</label>
								<input id="template-name" bind:value={templateName} placeholder="例如：可编辑协作者" class="w-full rounded-lg border border-ink-800/70 bg-ink-950/40 px-3 py-1.5 text-sm text-ink-200 outline-none focus:border-accent-500/40" />
							</div>
							<div>
								<label class="mb-1 block text-xs text-ink-500" for="template-desc">说明（可选）</label>
								<input id="template-desc" bind:value={templateDesc} placeholder="用于书库弹窗的权限选择" class="w-full rounded-lg border border-ink-800/70 bg-ink-950/40 px-3 py-1.5 text-sm text-ink-200 outline-none focus:border-accent-500/40" />
							</div>
							<div class="flex items-end gap-2">
								<button
									onclick={() => createTemplateMutation.mutate()}
									disabled={!templateName.trim() || createTemplateMutation.isPending}
									class="rounded-lg bg-accent-500/20 px-3 py-1.5 text-sm font-medium text-accent-200 hover:bg-accent-500/30 disabled:opacity-50"
								>创建</button>
								<button onclick={resetTemplateForm} class="rounded-lg px-3 py-1.5 text-sm text-ink-400 hover:text-ink-200">取消</button>
							</div>
						</div>
						<div class="mt-3 flex flex-wrap gap-3 text-xs text-ink-300">
							<label class="inline-flex items-center gap-1.5">
								<input type="checkbox" checked={templateRead} onchange={(e) => { templateRead = e.currentTarget.checked; if (!templateRead) { templateWrite = false; templateManage = false; } }} class="accent-accent-500" />
								读取
							</label>
							<label class="inline-flex items-center gap-1.5">
								<input type="checkbox" checked={templateWrite} onchange={(e) => { templateWrite = e.currentTarget.checked; if (templateWrite) templateRead = true; if (!templateWrite) templateManage = false; }} class="accent-accent-500" />
								写入
							</label>
							<label class="inline-flex items-center gap-1.5">
								<input type="checkbox" checked={templateManage} onchange={(e) => { templateManage = e.currentTarget.checked; if (templateManage) { templateRead = true; templateWrite = true; } }} class="accent-accent-500" />
								管理
							</label>
						</div>
					</div>
				{/if}

				{#if permissionTemplates.isLoading}
					<div class="grid gap-3 md:grid-cols-3">
						{#each Array(3) as _}
							<div class="h-28 rounded-xl bg-ink-900/50 animate-pulse"></div>
						{/each}
					</div>
				{:else if permissionTemplates.data && permissionTemplates.data.length > 0}
					<div class="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
						{#each permissionTemplates.data as template}
							<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
								{#if editingTemplateId === template.id}
									<div class="space-y-3">
										<div class="grid gap-2 sm:grid-cols-2">
											<div>
												<label class="mb-1 block text-xs text-ink-500" for="edit-template-name-{template.id}">名称</label>
												<input id="edit-template-name-{template.id}" bind:value={editTemplateName} class="w-full rounded-lg border border-ink-800/70 bg-ink-950/40 px-3 py-1.5 text-sm text-ink-200 outline-none focus:border-accent-500/40" />
											</div>
											<div>
												<label class="mb-1 block text-xs text-ink-500" for="edit-template-desc-{template.id}">说明</label>
												<input id="edit-template-desc-{template.id}" bind:value={editTemplateDesc} class="w-full rounded-lg border border-ink-800/70 bg-ink-950/40 px-3 py-1.5 text-sm text-ink-200 outline-none focus:border-accent-500/40" />
											</div>
										</div>
										<div class="flex flex-wrap gap-3 text-xs text-ink-300">
											<label class="inline-flex items-center gap-1.5">
												<input type="checkbox" checked={editTemplateRead} onchange={(e) => { editTemplateRead = e.currentTarget.checked; if (!editTemplateRead) { editTemplateWrite = false; editTemplateManage = false; } }} class="accent-accent-500" />
												读取
											</label>
											<label class="inline-flex items-center gap-1.5">
												<input type="checkbox" checked={editTemplateWrite} onchange={(e) => { editTemplateWrite = e.currentTarget.checked; if (editTemplateWrite) editTemplateRead = true; if (!editTemplateWrite) editTemplateManage = false; }} class="accent-accent-500" />
												写入
											</label>
											<label class="inline-flex items-center gap-1.5">
												<input type="checkbox" checked={editTemplateManage} onchange={(e) => { editTemplateManage = e.currentTarget.checked; if (editTemplateManage) { editTemplateRead = true; editTemplateWrite = true; } }} class="accent-accent-500" />
												管理
											</label>
										</div>
										<div class="flex items-center gap-2">
											<button
												onclick={() => updateTemplateMutation.mutate(template.id)}
												disabled={!editTemplateName.trim() || updateTemplateMutation.isPending}
												class="inline-flex items-center gap-1 rounded-lg bg-accent-500/20 px-2.5 py-1 text-xs text-accent-200 hover:bg-accent-500/30 disabled:opacity-50"
											><Check class="h-3 w-3" /> 保存</button>
											<button onclick={() => editingTemplateId = null} class="inline-flex items-center gap-1 rounded-lg px-2 py-1 text-xs text-ink-400 hover:text-ink-200"><X class="h-3 w-3" /> 取消</button>
										</div>
									</div>
								{:else}
									<div class="flex items-start justify-between gap-3">
										<div class="min-w-0">
											<div class="flex items-center gap-2">
												<div class="truncate font-medium text-ink-100">{template.name}</div>
												{#if template.is_system}
													<span class="rounded-full bg-ink-800/80 px-1.5 py-0.5 text-[10px] text-ink-400">系统</span>
												{/if}
											</div>
											{#if template.description}
												<p class="mt-1 line-clamp-2 text-xs text-ink-500">{template.description}</p>
											{/if}
										</div>
										{#if !template.is_system}
											<div class="flex shrink-0 items-center gap-1">
												<button onclick={() => openTemplateEditor(template)} class="rounded p-1 text-ink-600 hover:bg-ink-800/60 hover:text-ink-200" title="编辑模板">
													<Pencil class="h-3.5 w-3.5" />
												</button>
												<button onclick={() => deleteTemplateMutation.mutate(template.id)} class="rounded p-1 text-ink-600 hover:bg-red-500/10 hover:text-red-400" title="删除模板">
													<Trash2 class="h-3.5 w-3.5" />
												</button>
											</div>
										{/if}
									</div>
									<div class="mt-3 flex flex-wrap gap-1.5">
										<span class="rounded bg-emerald-500/10 px-2 py-0.5 text-[11px] {template.can_read ? 'text-emerald-300' : 'text-ink-600'}">读</span>
										<span class="rounded bg-sky-500/10 px-2 py-0.5 text-[11px] {template.can_write ? 'text-sky-300' : 'text-ink-600'}">写</span>
										<span class="rounded bg-amber-500/10 px-2 py-0.5 text-[11px] {template.can_manage ? 'text-amber-300' : 'text-ink-600'}">管</span>
									</div>
								{/if}
							</div>
						{/each}
					</div>
				{:else}
					<p class="rounded-xl border border-dashed border-ink-800/60 px-4 py-6 text-center text-sm text-ink-500">
						还没有权限模板。创建模板后，可在书库授权面板中快速套用。
					</p>
				{/if}
			</div>

			<!-- User Groups -->
			<div class="space-y-3">
			<div class="flex items-center justify-between">
				<h2 class="flex items-center gap-2 text-lg font-semibold text-ink-100">
					<UsersRound class="h-4.5 w-4.5 text-accent-400" />
					用户组
					<span class="text-xs font-normal text-ink-500">将用户分组以便批量授予书库权限</span>
				</h2>
				<button
					onclick={() => showGroupForm = !showGroupForm}
					class="inline-flex items-center gap-1.5 rounded-lg bg-accent-500/15 px-3 py-1.5 text-xs font-medium text-accent-200 hover:bg-accent-500/25"
				>
					<Plus class="h-3.5 w-3.5" /> 新建用户组
				</button>
			</div>

				{#if showGroupForm}
					<div class="flex flex-wrap items-end gap-3 rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
						<div class="flex-1 min-w-[160px]">
							<label class="mb-1 block text-xs text-ink-500" for="grp-name">组名</label>
							<input id="grp-name" bind:value={newGroupName} placeholder="例如：家庭成员" class="w-full rounded-lg border border-ink-800/70 bg-ink-950/40 px-3 py-1.5 text-sm text-ink-200 outline-none focus:border-accent-500/40" />
					</div>
					<div class="flex-[2] min-w-[200px]">
							<label class="mb-1 block text-xs text-ink-500" for="grp-desc">描述（可选）</label>
							<input id="grp-desc" bind:value={newGroupDesc} placeholder="组的用途" class="w-full rounded-lg border border-ink-800/70 bg-ink-950/40 px-3 py-1.5 text-sm text-ink-200 outline-none focus:border-accent-500/40" />
						</div>
						<div>
							<span class="mb-1 block text-xs text-ink-500">颜色</span>
							<div class="flex gap-1">
								{#each groupColors as color}
									<button
										onclick={() => newGroupColor = color}
										class="h-7 w-7 rounded-full border-2 {newGroupColor === color ? 'border-white' : 'border-transparent'}"
										style="background-color: {groupColorStyles[color]}"
										aria-label="选择 {color} 用户组颜色"
									></button>
								{/each}
							</div>
						</div>
						<button
							onclick={() => createGroupMutation.mutate()}
							disabled={!newGroupName.trim() || createGroupMutation.isPending}
							class="rounded-lg bg-accent-500/20 px-3 py-1.5 text-sm font-medium text-accent-200 hover:bg-accent-500/30 disabled:opacity-50"
					>创建</button>
				</div>
			{/if}

			{#if groups.data && groups.data.length > 0}
					<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
						{#each groups.data as group}
							<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4 space-y-2">
								{#if editingGroupId === group.id}
									<div class="space-y-3">
										<div>
											<label class="mb-1 block text-xs text-ink-500" for="edit-group-name-{group.id}">组名</label>
											<input id="edit-group-name-{group.id}" bind:value={groupDraftName} class="w-full rounded-lg border border-ink-800/70 bg-ink-950/40 px-3 py-1.5 text-sm text-ink-200 outline-none focus:border-accent-500/40" />
										</div>
										<div>
											<label class="mb-1 block text-xs text-ink-500" for="edit-group-desc-{group.id}">描述</label>
											<input id="edit-group-desc-{group.id}" bind:value={groupDraftDesc} class="w-full rounded-lg border border-ink-800/70 bg-ink-950/40 px-3 py-1.5 text-sm text-ink-200 outline-none focus:border-accent-500/40" />
										</div>
										<div>
											<span class="mb-1 block text-xs text-ink-500">颜色</span>
											<div class="flex gap-1">
												{#each groupColors as color}
													<button
														onclick={() => groupDraftColor = color}
														class="h-7 w-7 rounded-full border-2 {groupDraftColor === color ? 'border-white' : 'border-transparent'}"
														style="background-color: {groupColorStyles[color]}"
														aria-label="选择 {color} 用户组颜色"
													></button>
												{/each}
											</div>
										</div>
										<div class="flex items-center gap-2">
											<button
												onclick={() => updateGroupMutation.mutate(group.id)}
												disabled={!groupDraftName.trim() || updateGroupMutation.isPending}
												class="inline-flex items-center gap-1 rounded-lg bg-accent-500/20 px-2.5 py-1 text-xs text-accent-200 hover:bg-accent-500/30 disabled:opacity-50"
											><Check class="h-3 w-3" /> 保存</button>
											<button onclick={() => editingGroupId = null} class="inline-flex items-center gap-1 rounded-lg px-2 py-1 text-xs text-ink-400 hover:text-ink-200"><X class="h-3 w-3" /> 取消</button>
										</div>
									</div>
								{:else}
									<div class="flex items-start justify-between gap-2">
										<div class="min-w-0">
											<div class="flex items-center gap-2">
												<span class="h-2.5 w-2.5 shrink-0 rounded-full" style="background-color: {groupColorStyles[group.color] ?? groupColorStyles.slate}"></span>
												<div class="font-medium text-ink-100 truncate">{group.name}</div>
											</div>
											{#if group.description}
												<div class="mt-1 text-xs text-ink-500 truncate">{group.description}</div>
											{/if}
										</div>
										<div class="flex shrink-0 items-center gap-1">
											<button
												onclick={() => openGroupEditor(group)}
												class="rounded p-1 text-ink-600 hover:bg-ink-800/60 hover:text-ink-200"
												title="编辑组"
											>
												<Pencil class="h-3.5 w-3.5" />
											</button>
											<button
												onclick={() => deleteGroupMutation.mutate(group.id)}
												class="rounded p-1 text-ink-600 hover:text-red-400 hover:bg-red-500/10"
												title="删除组"
											>
												<Trash2 class="h-3.5 w-3.5" />
											</button>
										</div>
									</div>
								{/if}
								<div class="text-xs text-ink-400">{group.member_count} 位成员</div>

							{#if editingMembersFor === group.id}
								<div class="max-h-48 space-y-1 overflow-y-auto rounded-lg border border-ink-800/50 bg-ink-950/40 p-2">
									{#each users.data ?? [] as u}
										<label class="flex items-center gap-2 rounded px-1.5 py-1 text-xs text-ink-300 hover:bg-ink-800/40 cursor-pointer">
											<input type="checkbox" checked={memberDraft.has(u.id)} onchange={() => toggleMember(u.id)} class="accent-accent-500" />
											<span class="truncate">{u.display_name ?? u.username}</span>
										</label>
									{/each}
								</div>
								<div class="flex items-center gap-2">
									<button
										onclick={() => setMembersMutation.mutate({ id: group.id, userIds: [...memberDraft] })}
										class="inline-flex items-center gap-1 rounded-lg bg-accent-500/20 px-2.5 py-1 text-xs text-accent-200 hover:bg-accent-500/30"
									><Check class="h-3 w-3" /> 保存</button>
									<button
										onclick={() => editingMembersFor = null}
										class="inline-flex items-center gap-1 rounded-lg px-2 py-1 text-xs text-ink-400 hover:text-ink-200"
									><X class="h-3 w-3" /> 取消</button>
								</div>
							{:else}
								<div class="flex flex-wrap gap-1">
									{#each group.member_ids.slice(0, 6) as mid}
										<span class="rounded bg-ink-800/60 px-1.5 py-0.5 text-[10px] text-ink-300">{userName(mid)}</span>
									{/each}
									{#if group.member_ids.length > 6}
										<span class="text-[10px] text-ink-500">+{group.member_ids.length - 6}</span>
									{/if}
								</div>
								<button
									onclick={() => openMemberEditor(group.id, group.member_ids)}
									class="inline-flex items-center gap-1 rounded-lg border border-ink-800/70 px-2.5 py-1 text-xs text-ink-300 hover:bg-ink-800/40"
								><Pencil class="h-3 w-3" /> 管理成员</button>
							{/if}
						</div>
					{/each}
				</div>
			{:else if !showGroupForm}
				<p class="rounded-xl border border-dashed border-ink-800/60 px-4 py-6 text-center text-sm text-ink-500">
					还没有用户组。创建一个组，把用户归类后即可在书库对话框中按组授权。
				</p>
			{/if}
		</div>
	{:else}
		<div class="text-center py-12 text-ink-500">
			<Users class="w-12 h-12 mx-auto mb-3 opacity-30" />
			<p>暂无用户</p>
		</div>
	{/if}
</div>
