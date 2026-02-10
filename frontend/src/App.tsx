import { FormEvent, useEffect, useMemo, useState } from "react";
import logoImage from "./logo.jpg";

// Extend Window interface for MetaMask
declare global {
  interface Window {
    ethereum?: {
      request: (args: { method: string; params?: any[] }) => Promise<any>;
      isMetaMask?: boolean;
      selectedAddress?: string;
    };
  }
}

type Campaign = {
  id: string;
  name: string;
  sponsor: string;
  target_roles: string[];
  target_tools: string[];
  required_task: string;
  subsidy_per_call_cents: number;
  budget_total_cents: number;
  budget_remaining_cents: number;
  query_urls: string[];
  active: boolean;
  created_at: string;
};

type Profile = {
  id: string;
  email: string;
  region: string;
  roles: string[];
  tools_used: string[];
  created_at: string;
};

type CreatorSummary = {
  total_events: number;
  success_events: number;
  success_rate: number;
  per_skill: Array<{
    skill_name: string;
    total_events: number;
    success_events: number;
    avg_duration_ms: number | null;
    last_seen_at: string;
  }>;
};

type CampaignForm = {
  name: string;
  sponsor: string;
  target_roles: string;
  target_tools: string;
  required_task: string;
  subsidy_per_call_cents: number;
  budget_cents: number;
};

type SponsoredApi = {
  id: string;
  name: string;
  sponsor: string;
  description: string | null;
  upstream_url: string;
  upstream_method: string;
  price_cents: number;
  budget_total_cents: number;
  budget_remaining_cents: number;
  active: boolean;
  created_at: string;
};

type PaymentRequired = {
  service: string;
  amount_cents: number;
  accepted_header: string;
  payment_required: string;
  message: string;
  next_step: string;
};

type ServiceRunResponse = {
  service: string;
  output: string;
  payment_mode: string;
  sponsored_by: string | null;
  tx_hash: string | null;
};

type SponsoredApiRunResponse = {
  api_id: string;
  payment_mode: string;
  sponsored_by: string | null;
  tx_hash: string | null;
  upstream_status: number;
  upstream_body: string;
};

const defaultCampaignForm: CampaignForm = {
  name: "",
  sponsor: "",
  target_roles: "developer",
  target_tools: "scraping",
  required_task: "signup_sponsor",
  subsidy_per_call_cents: 5,
  budget_cents: 500
};

function App() {
  const [campaigns, setCampaigns] = useState<Campaign[]>([]);
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [creator, setCreator] = useState<CreatorSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [createLoading, setCreateLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [form, setForm] = useState<CampaignForm>(defaultCampaignForm);
  const [selectedTab, setSelectedTab] = useState("All");
  const [darkMode, setDarkMode] = useState(() => {
    const saved = localStorage.getItem("darkMode");
    return saved ? JSON.parse(saved) : false;
  });
  const [isLoggedIn, setIsLoggedIn] = useState(false); // Start logged out (public dashboard)
  const [showProfile, setShowProfile] = useState(false);
  const [currentView, setCurrentView] = useState<"dashboard" | "create-campaign" | "login" | "caller">("dashboard");
  const [loginForm, setLoginForm] = useState({ email: "", password: "" });
  const [sponsoredApis, setSponsoredApis] = useState<SponsoredApi[]>([]);
  const [callerLoading, setCallerLoading] = useState(false);
  const [callerResult, setCallerResult] = useState<any>(null);
  const [callerError, setCallerError] = useState<string | null>(null);
  const [callerForm, setCallerForm] = useState({
    callType: "proxy" as "proxy" | "tool" | "sponsored-api",
    service: "",
    apiId: "",
    input: "",
    userId: ""
  });
  const [paymentRequired, setPaymentRequired] = useState<PaymentRequired | null>(null);

  async function fetchJson<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await fetch(`/api${path}`, {
      ...init,
      headers: {
        "content-type": "application/json",
        ...(init?.headers ?? {})
      }
    });

    if (!response.ok) {
      const message = await response.text();
      throw new Error(message || `Request failed (${response.status})`);
    }

    return response.json() as Promise<T>;
  }

  async function loadDashboard(silent = false) {
    setLoading(true);
    if (!silent) {
    setError(null);
    }

    try {
      const [campaignData, profileData, creatorData] = await Promise.all([
        fetchJson<Campaign[]>("/campaigns", { method: "GET" }),
        fetchJson<Profile[]>("/profiles", { method: "GET" }),
        fetchJson<CreatorSummary>("/creator/metrics", { method: "GET" })
      ]);
      setCampaigns(campaignData);
      setProfiles(profileData);
      setCreator(creatorData);
    } catch (err) {
      // Only show error if not silent mode (for user-initiated actions)
      if (!silent) {
      setError(err instanceof Error ? err.message : "Unknown error");
      } else {
        // Silent mode: just set empty data, don't show error
        setCampaigns([]);
        setProfiles([]);
        setCreator(null);
      }
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadDashboard(true); // Silent mode - don't show errors on initial load
    void loadSponsoredApis();
  }, []);

  async function loadSponsoredApis() {
    try {
      const apis = await fetchJson<SponsoredApi[]>("/sponsored-apis", { method: "GET" });
      setSponsoredApis(apis);
    } catch (err) {
      // Silent fail
    }
  }

  useEffect(() => {
    localStorage.setItem("darkMode", JSON.stringify(darkMode));
    document.documentElement.setAttribute("data-theme", darkMode ? "dark" : "light");
  }, [darkMode]);

  useEffect(() => {
    // Save login state to localStorage when it changes
    localStorage.setItem("isLoggedIn", JSON.stringify(isLoggedIn));
  }, [isLoggedIn]);

  const toggleDarkMode = () => {
    setDarkMode((prev: boolean) => !prev);
  };

  const handleLogin = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    // Simple login - in production, this would call an API
    if (loginForm.email && loginForm.password) {
      setIsLoggedIn(true);
      localStorage.setItem("isLoggedIn", "true");
      setShowProfile(true);
      setLoginForm({ email: "", password: "" });
      // If we were trying to create a campaign, go there after login
      if (currentView === "login") {
        setCurrentView("create-campaign");
      } else {
        setCurrentView("dashboard");
      }
    }
  };

  const handleLogout = () => {
    setIsLoggedIn(false);
    localStorage.setItem("isLoggedIn", "false");
    setShowProfile(false);
    setCurrentView("login"); // Show login page after logout
  };

  const handleWalletConnect = async () => {
    setError(null);
    
    try {
      // Check if MetaMask is installed
      if (typeof window.ethereum === "undefined") {
        setError("Please install MetaMask or another Web3 wallet to continue");
        return;
      }

      // Request account access
      const accounts = await window.ethereum.request({
        method: "eth_requestAccounts"
      });

      if (accounts.length === 0) {
        setError("No wallet accounts found. Please connect your wallet.");
        return;
      }

      // Get the connected address
      const address = accounts[0];
      console.log("Connected wallet:", address);

      // Create a message to sign for authentication
      const message = `Sign in to PayloadExchange\n\nWallet: ${address}\nTimestamp: ${Date.now()}`;
      
      // Convert message to hex (browser-compatible)
      const messageHex = "0x" + Array.from(new TextEncoder().encode(message))
        .map(b => b.toString(16).padStart(2, "0"))
        .join("");
      
      // Request signature for authentication
      const signature = await window.ethereum.request({
        method: "personal_sign",
        params: [messageHex, address]
      });

      if (!signature) {
        setError("Signature required for authentication. Please sign the message.");
        return;
      }

      console.log("Signature received:", signature);

      // Now sign in after successful wallet connection AND signature
      setIsLoggedIn(true);
      localStorage.setItem("isLoggedIn", "true");
      localStorage.setItem("walletAddress", address);
      localStorage.setItem("walletSignature", signature);
      setShowProfile(true);
      
      // If we were trying to create a campaign, go there after login
      if (currentView === "login") {
        setCurrentView("create-campaign");
      } else {
        setCurrentView("dashboard");
      }
      
      // Load dashboard data silently in background (don't show errors)
      void loadDashboard(true);
    } catch (err: any) {
      // Handle user rejection or other errors
      if (err.code === 4001) {
        setError("Request rejected. Please connect and sign to continue.");
      } else if (err.code === -32602) {
        setError("Invalid signature request. Please try again.");
      } else {
        setError(err.message || "Failed to connect wallet. Please try again.");
      }
    }
  };

  const handleBack = () => {
    // Go back to dashboard (public view, no login required)
    setCurrentView("dashboard");
  };

  const totals = useMemo(() => {
    const activeCampaigns = campaigns.filter((item) => item.active).length;
    const subsidyCents = campaigns.reduce(
      (acc, item) => acc + item.subsidy_per_call_cents,
      0
    );
    const budgetCents = campaigns.reduce(
      (acc, item) => acc + item.budget_remaining_cents,
      0
    );
    const totalPayouts = campaigns.reduce(
      (acc, item) => acc + item.budget_total_cents - item.budget_remaining_cents,
      0
    );

    return {
      activeCampaigns,
      subsidyDollars: (subsidyCents / 100).toFixed(2),
      budgetDollars: (budgetCents / 100).toFixed(2),
      profiles: profiles.length,
      creatorSuccessRate: ((creator?.success_rate ?? 0) * 100).toFixed(1),
      totalPayouts: (totalPayouts / 100).toFixed(2)
    };
  }, [campaigns, profiles.length, creator?.success_rate]);

  async function onCreateCampaign(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setCreateLoading(true);
    setError(null);

    try {
      await fetchJson<Campaign>("/campaigns", {
        method: "POST",
        body: JSON.stringify({
          ...form,
          target_roles: splitCsv(form.target_roles),
          target_tools: splitCsv(form.target_tools)
        })
      });
      setForm(defaultCampaignForm);
      // Go back to dashboard after successful creation
      setCurrentView("dashboard");
      // Reload dashboard data silently
      void loadDashboard(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unknown error");
    } finally {
      setCreateLoading(false);
    }
  }

  async function handleApiCall(paymentSignature?: string) {
    setCallerLoading(true);
    setCallerError(null);
    setCallerResult(null);
    setPaymentRequired(null);

    try {
      const userId = callerForm.userId || localStorage.getItem("walletAddress") || "00000000-0000-0000-0000-000000000000";
      let path = "";
      let body: any = {};

      if (callerForm.callType === "sponsored-api") {
        if (!callerForm.apiId) {
          throw new Error("Please select a sponsored API");
        }
        path = `/sponsored-apis/${callerForm.apiId}/run`;
        body = {
          caller: userId,
          input: callerForm.input ? JSON.parse(callerForm.input) : {}
        };
      } else {
        if (!callerForm.service) {
          throw new Error("Please enter a service name");
        }
        path = `/${callerForm.callType}/${callerForm.service}/run`;
        body = {
          user_id: userId,
          input: callerForm.input || ""
        };
      }

      const headers: Record<string, string> = {
        "content-type": "application/json"
      };

      if (paymentSignature) {
        headers["payment-signature"] = paymentSignature;
      }

      const response = await fetch(`/api${path}`, {
        method: "POST",
        headers,
        body: JSON.stringify(body)
      });

      if (response.status === 402) {
        // Payment required
        const paymentData = await response.json();
        setPaymentRequired(paymentData);
        setCallerError("Payment required to access this service");
        return;
      }

      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(errorText || `Request failed (${response.status})`);
      }

      const result = await response.json();
      setCallerResult(result);
      setPaymentRequired(null);
    } catch (err) {
      setCallerError(err instanceof Error ? err.message : "Unknown error");
    } finally {
      setCallerLoading(false);
    }
  }

  async function handlePayment() {
    if (!paymentRequired) return;

    try {
      // Decode payment requirement
      const paymentReqBase64 = paymentRequired.payment_required;
      const paymentReqJson = atob(paymentReqBase64);
      const paymentReqs = JSON.parse(paymentReqJson);
      const paymentReq = paymentReqs[0]; // X402PaymentRequirement

      if (typeof window.ethereum === "undefined") {
        setCallerError("Please install MetaMask or another Web3 wallet to make payments");
        return;
      }

      // Request account access
      const accounts = await window.ethereum.request({
        method: "eth_requestAccounts"
      });

      if (accounts.length === 0) {
        setCallerError("No wallet accounts found");
        return;
      }

      const address = accounts[0];

      // Create payment signature using X402 protocol
      // For now, we'll use a simplified approach - in production, you'd use the X402 SDK
      const message = `Pay ${paymentReq.maxAmountRequired} ${paymentReq.asset} to ${paymentReq.payTo} for ${paymentReq.description}`;
      const messageHex = "0x" + Array.from(new TextEncoder().encode(message))
        .map(b => b.toString(16).padStart(2, "0"))
        .join("");

      const signature = await window.ethereum.request({
        method: "personal_sign",
        params: [messageHex, address]
      });

      if (!signature) {
        setCallerError("Payment signature required");
        return;
      }

      // Retry API call with payment signature
      await handleApiCall(signature);
    } catch (err: any) {
      if (err.code === 4001) {
        setCallerError("Payment request rejected");
      } else {
        setCallerError(err.message || "Failed to process payment");
      }
    }
  }

  // Generate mock chart data
  const chartData = [65, 72, 68, 85, 78, 82, 90];
  const barData = [
    { label: "Prospects", value: 45, color: "#4A9EFF" },
    { label: "Total Sales", value: 78, color: "#79F8C6" },
    { label: "Prospects", value: 52, color: "#4A9EFF" },
    { label: "Total Sales", value: 85, color: "#79F8C6" },
    { label: "Prospects", value: 38, color: "#4A9EFF" },
    { label: "Total Sales", value: 92, color: "#79F8C6" }
  ];

  return (
    <div className="dashboard">
      <header className="header">
        <div className="header-left">
          <div className="logo" onClick={() => setCurrentView("dashboard")} style={{ cursor: "pointer" }}>
            <img src={logoImage} alt="PayloadExchange" className="logo-icon" />
            <span className="logo-text">PayloadExchange</span>
          </div>
        </div>
        <div className="header-right">
          <button className="icon-btn">
            <span className="notification-dot"></span>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9"></path>
              <path d="M13.73 21a2 2 0 0 1-3.46 0"></path>
            </svg>
          </button>
          <div className="date-badge">
            <span>Mon, Feb 9</span>
            <span className="badge">12</span>
          </div>
          <button className="icon-btn">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <circle cx="11" cy="11" r="8"></circle>
              <path d="m21 21-4.35-4.35"></path>
            </svg>
          </button>
          <button className="icon-btn" onClick={toggleDarkMode} title="Toggle dark mode">
            {darkMode ? (
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <circle cx="12" cy="12" r="5"></circle>
                <line x1="12" y1="1" x2="12" y2="3"></line>
                <line x1="12" y1="21" x2="12" y2="23"></line>
                <line x1="4.22" y1="4.22" x2="5.64" y2="5.64"></line>
                <line x1="18.36" y1="18.36" x2="19.78" y2="19.78"></line>
                <line x1="1" y1="12" x2="3" y2="12"></line>
                <line x1="21" y1="12" x2="23" y2="12"></line>
                <line x1="4.22" y1="19.78" x2="5.64" y2="18.36"></line>
                <line x1="18.36" y1="5.64" x2="19.78" y2="4.22"></line>
              </svg>
            ) : (
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"></path>
              </svg>
            )}
          </button>
          <button 
            className="avatar" 
            onClick={() => {
              if (isLoggedIn) {
                setShowProfile(!showProfile);
              } else {
                setCurrentView("login");
              }
            }}
            title={isLoggedIn ? "Profile" : "Login"}
          >
            <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"></path>
              <circle cx="12" cy="7" r="4"></circle>
            </svg>
          </button>
          {isLoggedIn && (
            <button 
              className="logout-btn" 
              onClick={handleLogout}
              title="Logout"
            >
              Logout
            </button>
          )}
        </div>
      </header>

      {currentView === "login" ? (
        /* Login Page */
        <div className="login-page">
          <button 
            className="back-button"
            onClick={handleBack}
            title="Go back"
          >
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M19 12H5"></path>
              <path d="M12 19l-7-7 7-7"></path>
            </svg>
            Back
          </button>
          <div className="login-container">
            <div className="login-card">
              <div className="login-header">
                <div className="login-logo" onClick={() => setCurrentView("dashboard")}>
                  <img src={logoImage} alt="PayloadExchange" className="logo-icon-large" />
                  <h1>PayloadExchange</h1>
                </div>
                <p className="login-subtitle">Sign in to manage your campaigns</p>
              </div>
              
              <form className="login-form" onSubmit={handleLogin}>
                <div className="form-group">
                  <label>Email Address</label>
                  <input
                    type="email"
                    required
                    value={loginForm.email}
                    onChange={(e) => setLoginForm((prev) => ({ ...prev, email: e.target.value }))}
                    placeholder="you@example.com"
                  />
                </div>
                
                <div className="form-group">
                  <label>Password</label>
                  <input
                    type="password"
                    required
                    value={loginForm.password}
                    onChange={(e) => setLoginForm((prev) => ({ ...prev, password: e.target.value }))}
                    placeholder="Enter your password"
                  />
                </div>
                
                <div className="login-options">
                  <label className="checkbox-label">
                    <input type="checkbox" />
                    <span>Remember me</span>
                  </label>
                  <a href="#" className="forgot-link">Forgot password?</a>
                </div>
                
                <button type="submit" className="login-submit-btn">
                  Sign In
                </button>
                
                <div className="login-divider">
                  <span>or</span>
                </div>
                
                <button type="button" className="wallet-login-btn" onClick={handleWalletConnect}>
                  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <rect x="1" y="4" width="22" height="16" rx="2" ry="2"></rect>
                    <line x1="1" y1="10" x2="23" y2="10"></line>
                  </svg>
                  Connect Wallet
                </button>
                
                <p className="login-footer">
                  Don't have an account? <a href="#">Sign up</a>
                </p>
              </form>
            </div>
          </div>
        </div>
      ) : currentView === "create-campaign" ? (
        /* Create Campaign Page */
        <main className="main-content">
          <div className="create-campaign-page">
            <div className="page-header">
              <button 
                className="back-button-inline"
                onClick={() => setCurrentView("dashboard")}
                title="Back to dashboard"
              >
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M19 12H5"></path>
                  <path d="M12 19l-7-7 7-7"></path>
                </svg>
                Back to Dashboard
              </button>
              <h2>Create New Campaign</h2>
              <p>Launch a payout stream for target developer segments</p>
            </div>

            <div className="card create-campaign-card">
              <div className="card-content">
                {error && <div className="error-message">{error}</div>}
                <div className="profile-card">
                  <div className="profile-avatar">
                    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"></path>
                      <circle cx="12" cy="7" r="4"></circle>
                    </svg>
                  </div>
                  <div className="profile-details">
                    <h4>Your Profile</h4>
                    <div className="profile-stats">
                      <span>Active: {totals.activeCampaigns}</span>
                      <span>Total: {campaigns.length}</span>
                      <span>Budget: ${totals.budgetDollars}</span>
                    </div>
                  </div>
                </div>
                <form className="campaign-form" onSubmit={onCreateCampaign}>
                  <div className="form-group">
                    <label>Campaign Name</label>
                    <input
                      required
                      value={form.name}
                      onChange={(e) => setForm((prev) => ({ ...prev, name: e.target.value }))}
                      placeholder="Enter campaign name"
                    />
                  </div>
                  <div className="form-group">
                    <label>Sponsor</label>
                    <input
                      required
                      value={form.sponsor}
                      onChange={(e) => setForm((prev) => ({ ...prev, sponsor: e.target.value }))}
                      placeholder="Enter sponsor name"
                    />
                  </div>
                  <div className="form-group">
                    <label>Target Roles (comma-separated)</label>
                    <input
                      value={form.target_roles}
                      onChange={(e) => setForm((prev) => ({ ...prev, target_roles: e.target.value }))}
                      placeholder="developer, designer, etc."
                    />
                  </div>
                  <div className="form-group">
                    <label>Target Tools (comma-separated)</label>
                    <input
                      value={form.target_tools}
                      onChange={(e) => setForm((prev) => ({ ...prev, target_tools: e.target.value }))}
                      placeholder="scraping, search, etc."
                    />
                  </div>
                  <div className="form-group">
                    <label>Required Task</label>
                    <input
                      required
                      value={form.required_task}
                      onChange={(e) => setForm((prev) => ({ ...prev, required_task: e.target.value }))}
                      placeholder="signup_sponsor"
                    />
                  </div>
                  <div className="form-row">
                    <div className="form-group">
                      <label>Subsidy / Call (cents)</label>
                      <input
                        required
                        type="number"
                        min={1}
                        value={form.subsidy_per_call_cents}
                        onChange={(e) =>
                          setForm((prev) => ({
                            ...prev,
                            subsidy_per_call_cents: Number(e.target.value)
                          }))
                        }
                      />
                    </div>
                    <div className="form-group">
                      <label>Budget (cents)</label>
                      <input
                        required
                        type="number"
                        min={1}
                        value={form.budget_cents}
                        onChange={(e) =>
                          setForm((prev) => ({
                            ...prev,
                            budget_cents: Number(e.target.value)
                          }))
                        }
                      />
                    </div>
                  </div>
                  <button type="submit" className="submit-btn" disabled={createLoading}>
                    {createLoading ? "Creating..." : "Create Campaign"}
                  </button>
                </form>
              </div>
            </div>
          </div>
        </main>
      ) : currentView === "caller" ? (
        /* Caller Page */
        <main className="main-content">
          <div className="create-campaign-page">
            <div className="page-header">
              <button 
                className="back-button-inline"
                onClick={() => setCurrentView("dashboard")}
                title="Back to dashboard"
              >
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M19 12H5"></path>
                  <path d="M12 19l-7-7 7-7"></path>
                </svg>
                Back to Dashboard
              </button>
              <h2>API Caller</h2>
              <p>Call APIs and handle payments automatically</p>
            </div>

            <div className="card create-campaign-card">
              <div className="card-content">
                <form className="campaign-form" onSubmit={(e) => { e.preventDefault(); void handleApiCall(); }}>
                  <div className="form-group">
                    <label>Call Type</label>
                    <select
                      value={callerForm.callType}
                      onChange={(e) => setCallerForm((prev) => ({ ...prev, callType: e.target.value as any }))}
                    >
                      <option value="proxy">Proxy Service</option>
                      <option value="tool">Direct Tool</option>
                      <option value="sponsored-api">Sponsored API</option>
                    </select>
                  </div>

                  {callerForm.callType === "sponsored-api" ? (
                    <div className="form-group">
                      <label>Sponsored API</label>
                      <select
                        value={callerForm.apiId}
                        onChange={(e) => setCallerForm((prev) => ({ ...prev, apiId: e.target.value }))}
                      >
                        <option value="">Select an API</option>
                        {sponsoredApis.map((api) => (
                          <option key={api.id} value={api.id}>
                            {api.name} - ${(api.price_cents / 100).toFixed(2)} per call
                            {api.active && api.budget_remaining_cents > 0 ? " (Sponsored)" : " (Paid)"}
                          </option>
                        ))}
                      </select>
                    </div>
                  ) : (
                    <div className="form-group">
                      <label>Service Name</label>
                      <input
                        required
                        value={callerForm.service}
                        onChange={(e) => setCallerForm((prev) => ({ ...prev, service: e.target.value }))}
                        placeholder="e.g., scraping, design, storage"
                      />
                    </div>
                  )}

                  <div className="form-group">
                    <label>User ID (optional, defaults to wallet address)</label>
                    <input
                      value={callerForm.userId}
                      onChange={(e) => setCallerForm((prev) => ({ ...prev, userId: e.target.value }))}
                      placeholder="Leave empty to use wallet address"
                    />
                  </div>

                  <div className="form-group">
                    <label>Input {callerForm.callType === "sponsored-api" ? "(JSON)" : "(text)"}</label>
                    <textarea
                      rows={6}
                      value={callerForm.input}
                      onChange={(e) => setCallerForm((prev) => ({ ...prev, input: e.target.value }))}
                      placeholder={callerForm.callType === "sponsored-api" ? '{"key": "value"}' : "Enter input text"}
                    />
                  </div>

                  {paymentRequired && (
                    <div className="payment-required-box">
                      <div className="payment-header">
                        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <circle cx="12" cy="12" r="10"></circle>
                          <line x1="12" y1="8" x2="12" y2="12"></line>
                          <line x1="12" y1="16" x2="12.01" y2="16"></line>
                        </svg>
                        <h4>Payment Required</h4>
                      </div>
                      <div className="payment-details">
                        <p><strong>Service:</strong> {paymentRequired.service}</p>
                        <p><strong>Amount:</strong> ${(paymentRequired.amount_cents / 100).toFixed(2)}</p>
                        <p><strong>Message:</strong> {paymentRequired.message}</p>
                        <p><strong>Next Step:</strong> {paymentRequired.next_step}</p>
                      </div>
                      <button
                        type="button"
                        className="wallet-login-btn"
                        onClick={handlePayment}
                        disabled={callerLoading}
                      >
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <rect x="1" y="4" width="22" height="16" rx="2" ry="2"></rect>
                          <line x1="1" y1="10" x2="23" y2="10"></line>
                        </svg>
                        Pay with Wallet
                      </button>
                    </div>
                  )}

                  {callerError && <div className="error-message">{callerError}</div>}

                  <button type="submit" className="submit-btn" disabled={callerLoading}>
                    {callerLoading ? "Calling..." : "Call API"}
                  </button>
                </form>

                {callerResult && (
                  <div className="caller-result">
                    <h4>Result</h4>
                    <div className="result-box">
                      <pre>{JSON.stringify(callerResult, null, 2)}</pre>
                    </div>
                    {callerResult.payment_mode && (
                      <div className="payment-info">
                        <p><strong>Payment Mode:</strong> {callerResult.payment_mode}</p>
                        {callerResult.sponsored_by && (
                          <p><strong>Sponsored By:</strong> {callerResult.sponsored_by}</p>
                        )}
                        {callerResult.tx_hash && (
                          <p><strong>Transaction Hash:</strong> {callerResult.tx_hash}</p>
                        )}
                      </div>
                    )}
                  </div>
                )}
              </div>
            </div>
          </div>
        </main>
      ) : currentView === "caller" ? (
        /* Caller Page */
        <main className="main-content">
          <div className="create-campaign-page">
            <div className="page-header">
              <button 
                className="back-button-inline"
                onClick={() => setCurrentView("dashboard")}
                title="Back to dashboard"
              >
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M19 12H5"></path>
                  <path d="M12 19l-7-7 7-7"></path>
                </svg>
                Back to Dashboard
              </button>
              <h2>API Caller</h2>
              <p>Call APIs and handle payments automatically</p>
            </div>

            <div className="card create-campaign-card">
              <div className="card-content">
                <form className="campaign-form" onSubmit={(e) => { e.preventDefault(); void handleApiCall(); }}>
                  <div className="form-group">
                    <label>Call Type</label>
                    <select
                      value={callerForm.callType}
                      onChange={(e) => setCallerForm((prev) => ({ ...prev, callType: e.target.value as any }))}
                    >
                      <option value="proxy">Proxy Service</option>
                      <option value="tool">Direct Tool</option>
                      <option value="sponsored-api">Sponsored API</option>
                    </select>
                  </div>

                  {callerForm.callType === "sponsored-api" ? (
                    <div className="form-group">
                      <label>Sponsored API</label>
                      <select
                        value={callerForm.apiId}
                        onChange={(e) => setCallerForm((prev) => ({ ...prev, apiId: e.target.value }))}
                      >
                        <option value="">Select an API</option>
                        {sponsoredApis.map((api) => (
                          <option key={api.id} value={api.id}>
                            {api.name} - ${(api.price_cents / 100).toFixed(2)} per call
                            {api.active && api.budget_remaining_cents > 0 ? " (Sponsored)" : " (Paid)"}
                          </option>
                        ))}
                      </select>
                    </div>
                  ) : (
                    <div className="form-group">
                      <label>Service Name</label>
                      <input
                        required
                        value={callerForm.service}
                        onChange={(e) => setCallerForm((prev) => ({ ...prev, service: e.target.value }))}
                        placeholder="e.g., scraping, design, storage"
                      />
                    </div>
                  )}

                  <div className="form-group">
                    <label>User ID (optional, defaults to wallet address)</label>
                    <input
                      value={callerForm.userId}
                      onChange={(e) => setCallerForm((prev) => ({ ...prev, userId: e.target.value }))}
                      placeholder="Leave empty to use wallet address"
                    />
                  </div>

                  <div className="form-group">
                    <label>Input {callerForm.callType === "sponsored-api" ? "(JSON)" : "(text)"}</label>
                    <textarea
                      rows={6}
                      value={callerForm.input}
                      onChange={(e) => setCallerForm((prev) => ({ ...prev, input: e.target.value }))}
                      placeholder={callerForm.callType === "sponsored-api" ? '{"key": "value"}' : "Enter input text"}
                    />
                  </div>

                  {paymentRequired && (
                    <div className="payment-required-box">
                      <div className="payment-header">
                        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <circle cx="12" cy="12" r="10"></circle>
                          <line x1="12" y1="8" x2="12" y2="12"></line>
                          <line x1="12" y1="16" x2="12.01" y2="16"></line>
                        </svg>
                        <h4>Payment Required</h4>
                      </div>
                      <div className="payment-details">
                        <p><strong>Service:</strong> {paymentRequired.service}</p>
                        <p><strong>Amount:</strong> ${(paymentRequired.amount_cents / 100).toFixed(2)}</p>
                        <p><strong>Message:</strong> {paymentRequired.message}</p>
                        <p><strong>Next Step:</strong> {paymentRequired.next_step}</p>
                      </div>
                      <button
                        type="button"
                        className="wallet-login-btn"
                        onClick={handlePayment}
                        disabled={callerLoading}
                      >
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <rect x="1" y="4" width="22" height="16" rx="2" ry="2"></rect>
                          <line x1="1" y1="10" x2="23" y2="10"></line>
                        </svg>
                        Pay with Wallet
                      </button>
                    </div>
                  )}

                  {callerError && <div className="error-message">{callerError}</div>}

                  <button type="submit" className="submit-btn" disabled={callerLoading}>
                    {callerLoading ? "Calling..." : "Call API"}
                  </button>
                </form>

                {callerResult && (
                  <div className="caller-result">
                    <h4>Result</h4>
                    <div className="result-box">
                      <pre>{JSON.stringify(callerResult, null, 2)}</pre>
                    </div>
                    {callerResult.payment_mode && (
                      <div className="payment-info">
                        <p><strong>Payment Mode:</strong> {callerResult.payment_mode}</p>
                        {callerResult.sponsored_by && (
                          <p><strong>Sponsored By:</strong> {callerResult.sponsored_by}</p>
                        )}
                        {callerResult.tx_hash && (
                          <p><strong>Transaction Hash:</strong> {callerResult.tx_hash}</p>
                        )}
                      </div>
                    )}
                  </div>
                )}
              </div>
            </div>
          </div>
        </main>
      ) : (
        /* Dashboard */
        <main className="main-content">
          <div className="dashboard-grid">
          {/* Sales Ratings Card */}
          <div className="card">
            <div className="card-header">
              <div className="card-title">
                <span className="card-icon">
                  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon>
                  </svg>
                </span>
                <h3>Campaign Performance</h3>
              </div>
              <div className="tabs">
                {["All", "Active", "Paused"].map((tab) => (
                  <button
                    key={tab}
                    className={selectedTab === tab ? "tab active" : "tab"}
                    onClick={() => setSelectedTab(tab)}
                  >
                    {tab}
                  </button>
                ))}
              </div>
            </div>
            <div className="card-content">
              <div className="metric-highlight">
                <span className="metric-value">34%</span>
                <span className="metric-text">rating increases every week</span>
              </div>
              <div className="bar-chart">
                {barData.map((bar, i) => (
                  <div key={i} className="bar-group">
                    <div
                      className="bar"
                      style={{
                        height: `${bar.value}%`,
                        backgroundColor: bar.color
                      }}
                    ></div>
                  </div>
                ))}
              </div>
              <div className="chart-legend">
                <span className="legend-item">
                  <span className="legend-dot" style={{ backgroundColor: "#79F8C6" }}></span>
                  Total Sales
                </span>
                <span className="legend-item">
                  <span className="legend-dot" style={{ backgroundColor: "#4A9EFF" }}></span>
                  Prospects
                </span>
              </div>
              <div className="stat-box">
                <div className="stat-value">${totals.totalPayouts}</div>
                <div className="stat-label">7 Days</div>
                <div className="stat-change positive">+72.9%</div>
                <div className="stat-note">better than last week</div>
              </div>
            </div>
          </div>

          {/* Analytics Card */}
          <div className="card">
            <div className="card-header">
              <div className="card-title">
                <span className="card-icon">
                  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <polyline points="22 6 13.5 15.5 8.5 10.5 2 18"></polyline>
                    <polyline points="16 6 22 6 22 12"></polyline>
                  </svg>
                </span>
                <h3>Analytics</h3>
              </div>
              <div className="card-actions">
                <button className="filter-btn active">Weekly</button>
                <button className="filter-btn">Orders ▼</button>
                <button className="icon-btn-small">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <circle cx="12" cy="12" r="1"></circle>
                  <circle cx="19" cy="12" r="1"></circle>
                  <circle cx="5" cy="12" r="1"></circle>
                </svg>
              </button>
              </div>
            </div>
            <div className="card-content">
              <div className="analytics-metrics">
                <div className="metric-item">
                  <span className="metric-label">Rate</span>
                  <span className="metric-number positive">0.75%</span>
                  <span className="metric-change positive">↑ 13%</span>
                </div>
                <div className="metric-item">
                  <span className="metric-label">Sales</span>
                  <span className="metric-number negative">-$2,480</span>
                  <span className="metric-change negative">↓ 0.4%</span>
                </div>
              </div>
              <div className="line-chart">
                <div className="chart-area">
                  {chartData.map((value, i) => (
                    <div key={i} className="chart-point" style={{ bottom: `${value}%` }}>
                      <div className="point-dot"></div>
                      {i === 2 && <div className="point-label">+34%</div>}
                    </div>
                  ))}
                  <svg className="chart-line" viewBox="0 0 200 100" preserveAspectRatio="none">
                    <polyline
                      points="0,35 33,28 66,32 100,15 133,22 166,18 200,10"
                      fill="none"
                      stroke="#4A9EFF"
                      strokeWidth="2"
                    />
                    <polygon
                      points="0,35 33,28 66,32 100,15 133,22 166,18 200,10 200,100 0,100"
                      fill="url(#gradient)"
                      opacity="0.2"
                    />
                    <defs>
                      <linearGradient id="gradient" x1="0%" y1="0%" x2="0%" y2="100%">
                        <stop offset="0%" stopColor="#4A9EFF" />
                        <stop offset="100%" stopColor="#4A9EFF" stopOpacity="0" />
                      </linearGradient>
                    </defs>
                  </svg>
                </div>
                <div className="chart-labels">
                  {["Jan", "Feb", "Mar", "Apr", "May", "Jun"].map((month) => (
                    <span key={month}>{month}</span>
                  ))}
                </div>
              </div>
            </div>
          </div>

          {/* Profit Card */}
          <div className="card">
            <div className="card-header">
              <div className="card-title">
                <h3>Profit</h3>
              </div>
              <button className="icon-btn-small">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <circle cx="12" cy="12" r="1"></circle>
                  <circle cx="19" cy="12" r="1"></circle>
                  <circle cx="5" cy="12" r="1"></circle>
                </svg>
              </button>
            </div>
            <div className="card-content">
              <div className="donut-chart">
                <div className="donut-center">
                  <span className="donut-value">24%</span>
                  <span className="donut-label">from yesterday</span>
                </div>
                <svg className="donut-svg" viewBox="0 0 120 120">
                  <circle
                    cx="60"
                    cy="60"
                    r="50"
                    fill="none"
                    stroke="#E5E7EB"
                    strokeWidth="12"
                  />
                  <circle
                    cx="60"
                    cy="60"
                    r="50"
                    fill="none"
                    stroke="#79F8C6"
                    strokeWidth="12"
                    strokeDasharray={`${24 * 3.14} ${100 * 3.14}`}
                    strokeDashoffset="0"
                    transform="rotate(-90 60 60)"
                  />
                  <circle
                    cx="60"
                    cy="60"
                    r="50"
                    fill="none"
                    stroke="#4A9EFF"
                    strokeWidth="12"
                    strokeDasharray={`${36 * 3.14} ${100 * 3.14}`}
                    strokeDashoffset={`-${24 * 3.14}`}
                    transform="rotate(-90 60 60)"
                  />
                </svg>
              </div>
              <div className="profit-text">
                Profit is 36% More than last week
              </div>
              <div className="profit-legend">
                <div className="legend-row">
                  <span className="legend-dot" style={{ backgroundColor: "#79F8C6" }}></span>
                  <span>Total Profit per day</span>
                </div>
                <div className="legend-row">
                  <span className="legend-dot" style={{ backgroundColor: "#4A9EFF" }}></span>
                  <span>For Week</span>
                </div>
              </div>
            </div>
          </div>

          {/* Promotional Banner */}
          <div className="card promotional-banner">
            <div className="banner-content">
              <div className="banner-text">
                <div className="banner-vertical">UNLOCK YOUR GROWTH</div>
                <h2>Power Your Business with Sponsored Compute Insights!</h2>
                <img src={logoImage} alt="PayloadExchange" className="banner-logo" />
              </div>
            </div>
          </div>

          {/* Stock Product / Campaigns Card */}
          <div className="card">
            <div className="card-header">
              <div className="card-title">
                <span className="card-icon">
                  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect>
                    <line x1="9" y1="3" x2="9" y2="21"></line>
                    <line x1="3" y1="9" x2="21" y2="9"></line>
                  </svg>
                </span>
                <h3>Active Campaigns</h3>
              </div>
              <div className="card-actions">
                <button className="filter-btn">Weekly ▼</button>
                <button className="icon-btn-small">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <circle cx="12" cy="12" r="1"></circle>
                  <circle cx="19" cy="12" r="1"></circle>
                  <circle cx="5" cy="12" r="1"></circle>
                </svg>
              </button>
              </div>
            </div>
            <div className="card-content">
              <div className="campaigns-grid">
                {loading ? (
                  <div className="loading">Loading campaigns...</div>
                ) : campaigns.length === 0 ? (
                  <div className="empty-state">No campaigns yet</div>
                ) : (
                  campaigns.slice(0, 4).map((campaign, i) => (
                    <div key={campaign.id} className="campaign-item">
                      <div className="campaign-name">{campaign.name}</div>
                      <div className="campaign-bars">
                        {[1, 2, 3, 4, 5].map((bar) => (
                          <div
                            key={bar}
                            className="campaign-bar"
                            style={{
                              backgroundColor: `rgba(74, 158, 255, ${0.3 + (i * 0.15)})`,
                              height: `${20 + bar * 15}%`
                            }}
                          ></div>
                        ))}
                      </div>
                    </div>
                  ))
                )}
              </div>
              <div className="campaign-legend">
                <span>Budget Usage</span>
                <div className="legend-scale">
                  <span>Low</span>
                  <span>Medium</span>
                  <span>High</span>
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* Campaigns Table Card */}
        <div className="card full-width">
          <div className="card-header">
            <div className="card-title">
              <h3>Campaign Details</h3>
            </div>
            <div className="card-actions">
              <button 
                className="primary-btn" 
                onClick={() => {
                  if (isLoggedIn) {
                    setCurrentView("create-campaign");
                  } else {
                    setCurrentView("login");
                  }
                }}
              >
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <line x1="12" y1="5" x2="12" y2="19"></line>
                  <line x1="5" y1="12" x2="19" y2="12"></line>
                </svg>
                Create Campaign
              </button>
              <button 
                className="primary-btn" 
                onClick={() => setCurrentView("caller")}
                style={{ marginLeft: "8px" }}
              >
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z"></path>
                </svg>
                API Caller
              </button>
              <button className="ghost-btn" onClick={() => void loadDashboard(false)}>
                Refresh
              </button>
            </div>
          </div>
          <div className="table-container">
              <table>
                <thead>
                  <tr>
                    <th>Name</th>
                    <th>Sponsor</th>
                    <th>Targets</th>
                    <th>Subsidy</th>
                    <th>Budget Left</th>
                    <th>Status</th>
                  </tr>
                </thead>
                <tbody>
                  {loading ? (
                    <tr>
                      <td colSpan={6}>Loading dashboard...</td>
                    </tr>
                  ) : campaigns.length === 0 ? (
                    <tr>
                      <td colSpan={6}>No campaigns yet.</td>
                    </tr>
                  ) : (
                    campaigns.map((campaign) => (
                      <tr key={campaign.id}>
                        <td>{campaign.name}</td>
                        <td>{campaign.sponsor}</td>
                        <td>
                          {campaign.target_roles.join(", ")} / {campaign.target_tools.join(", ")}
                        </td>
                        <td>${(campaign.subsidy_per_call_cents / 100).toFixed(2)}</td>
                        <td>${(campaign.budget_remaining_cents / 100).toFixed(2)}</td>
                        <td>
                        <span className={campaign.active ? "status-badge active" : "status-badge paused"}>
                            {campaign.active ? "ACTIVE" : "PAUSED"}
                          </span>
                        </td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>
            </div>

          {/* User Profile Section - Only shown when logged in */}
          {isLoggedIn && showProfile && (
          <div className="card profile-card">
            <div className="card-header">
              <div className="card-title">
                <h3>My Profile</h3>
              </div>
              <button 
                className="icon-btn-small"
                onClick={() => setShowProfile(false)}
                title="Close"
              >
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <line x1="18" y1="6" x2="6" y2="18"></line>
                  <line x1="6" y1="6" x2="18" y2="18"></line>
                </svg>
              </button>
            </div>
            <div className="card-content">
              {/* Profile Header */}
              <div className="profile-section">
                <div className="profile-header">
                  <div className="profile-avatar-large">
                    <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"></path>
                      <circle cx="12" cy="7" r="4"></circle>
                    </svg>
                  </div>
                  <div className="profile-info">
                    <h4>Campaign Creator</h4>
                    <p>Manage your sponsored campaigns and track performance</p>
                  </div>
                </div>
                <div className="profile-stats">
                  <div className="profile-stat">
                    <span className="stat-number">{totals.activeCampaigns}</span>
                    <span className="stat-label">Active</span>
                  </div>
                  <div className="profile-stat">
                    <span className="stat-number">{campaigns.length}</span>
                    <span className="stat-label">Total</span>
                  </div>
                  <div className="profile-stat">
                    <span className="stat-number">${totals.budgetDollars}</span>
                    <span className="stat-label">Budget</span>
                  </div>
                </div>
              </div>

              {/* Create Campaign Button */}
              <div className="profile-form-section">
                <div className="section-divider">
                  <h4>Create New Campaign</h4>
                  <p>Launch a payout stream for target developer segments</p>
                </div>
                <button 
                  className="primary-btn-large" 
                  onClick={() => {
                    if (isLoggedIn) {
                      setCurrentView("create-campaign");
                    } else {
                      setCurrentView("login");
                    }
                  }}
                >
                  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <line x1="12" y1="5" x2="12" y2="19"></line>
                    <line x1="5" y1="12" x2="19" y2="12"></line>
                  </svg>
                  Go to Create Campaign
                </button>
              </div>
            </div>
            </div>
          )}
        </main>
      )}
    </div>
  );
}

function splitCsv(raw: string): string[] {
  return raw
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean);
}

export default App;
