/**
 * Auth store — manages user authentication state.
 * Uses Svelte 5 runes for reactive state.
 */
import { api } from '$services/api';

interface AuthUser {
	id: string;
	username: string;
	email?: string;
	display_name?: string;
	avatar_url?: string;
	avatar_path?: string;
	role?: string;
}

class AuthStore {
	user = $state<AuthUser | null>(null);
	loading = $state(true);
	isAuthenticated = $derived(this.user !== null);

	async init() {
		try {
			const me = await api.getMe();
			this.user = me;
		} catch {
			// Token might be expired — try refresh
			try {
				await api.refreshToken();
				const me = await api.getMe();
				this.user = me;
			} catch {
				this.user = null;
			}
		} finally {
			this.loading = false;
		}
	}

	async login(username: string, password: string) {
		const result = await api.login({ username, password });
		this.user = result.user;
		return result;
	}

	async logout() {
		try {
			await api.logout();
		} finally {
			this.user = null;
		}
	}

	async refresh() {
		try {
			await api.refreshToken();
			const me = await api.getMe();
			this.user = me;
		} catch {
			this.user = null;
		}
	}
}

export const auth = new AuthStore();
