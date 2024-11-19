fetch("/pageview", {
	method: "POST",
	headers: { "Content-Type": "application/json" },
	body: JSON.stringify({ path: window.location.pathname }),
});
