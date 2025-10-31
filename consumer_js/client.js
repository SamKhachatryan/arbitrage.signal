const wsUri = "ws://localhost:4010";

const websocket = new WebSocket(wsUri);

const diffMap = {};

let prev_package_count = 0;
let package_count = 0;

websocket.onopen = (event) => {
    console.log("WebSocket connection established:", event);
    setInterval(() => {
        console.log('Package got within second:', package_count - prev_package_count);
        prev_package_count = package_count;
    }, 1000);
};

websocket.onmessage = (event) => {
    package_count++;
    const parsed = JSON.parse(event.data);

    Object.entries(parsed).forEach(([pairName, pair]) => {
        const allExchangesMap = Object.entries(pair);

        allExchangesMap.forEach(([exchangeName, price]) => {
            const otherExchanges = allExchangesMap.filter(item => item.exchangeName !== exchangeName);
            const otherExchangesMod = otherExchanges.reduce((acc, [, price]) => acc + price * 10000, 0) / otherExchanges.length;

            diffMap[pairName] ??= {
                diff: 0,
                exchangeName: exchangeName,
            };

            // if (Math.abs(price * 10000 - otherExchangesMod) > diffMap[pairName].diff) {
                diffMap[pairName].diff = Math.abs(price * 10000 - otherExchangesMod);
                diffMap[pairName].exchangeName = exchangeName
            // }
        });
    });

    draw_diff_map();
};

const draw_diff_map = () => {
    const elements = document.getElementsByClassName('mod-diff-pair');
    Array.from(elements).forEach(item => document.body.removeChild(item));

    Object.entries(diffMap).forEach(([pairName, diff]) => {
        const p = document.createElement('p');
        p.className = 'mod-diff-pair';
        p.textContent = `(${diff.exchangeName}) ${pairName} - ${diff.diff / 10000}`;
        document.body.appendChild(p);
    });
}

websocket.onerror = (error) => {
    console.error("WebSocket error:", error);
};

websocket.onclose = (event) => {
    console.log("WebSocket connection closed:", event);
};

function sendMessage(message) {
    if (websocket.readyState === WebSocket.OPEN) {
        websocket.send(message);
    } else {
        console.warn("WebSocket is not open. Cannot send message.");
    }
}