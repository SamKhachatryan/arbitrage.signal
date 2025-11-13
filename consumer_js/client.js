// const wsUri = "ws://localhost:4010";
const wsUri = "ws://185.7.81.99:4010";

const websocket = new WebSocket(wsUri);
websocket.binaryType = "arraybuffer"; // important

const diffMap = {};

let prev_package_count = 0;
let package_count = 0;

websocket.onopen = (event) => {
    console.log("WebSocket connection established:", event);
    // setInterval(() => {
    //     console.log('Package got within 10 second:', package_count - prev_package_count);
    //     prev_package_count = package_count;
    // }, 10000);
};

const arbitrageThresholds = {
    "btc-usdt": 0.5,
    "eth-usdt": 0.6,
    "sol-usdt": 0.7,
    "doge-usdt": 0.8,
    "xrp-usdt": 0.7,
    "ton-usdt": 0.9,
    "ada-usdt": 0.6,
    "link-usdt": 0.7,
    "arb-usdt": 0.8,
    "op-usdt": 0.8,
    "ltc-usdt": 0.6,
    "bch-usdt": 0.7,
    "uni-usdt": 0.8,
    "avax-usdt": 0.8,
    "apt-usdt": 0.9,
    "near-usdt": 0.8,
    "matic-usdt": 0.7,
    "pepe-usdt": 1.2,
    "floki-usdt": 1.3,
    "sui-usdt": 0.9,
    "icp-usdt": 0.9,
    "xvs-usdt": 1.0,
    "ach-usdt": 1.1,
    "fet-usdt": 0.9,
    "rndr-usdt": 0.8,
    "enj-usdt": 0.9,
    "cfx-usdt": 0.5,
    "kas-usdt": 0.6,
    "mina-usdt": 1.0,
    "gala-usdt": 1.1,
    "blur-usdt": 1.2,
    "wojak-usdt": 1.3,
    "bnb-usdt": 0.5,
};


const reliabilityEnum = {
    not_reliable_at_all: 0,
    ultralow: 1,
    low: 2,
    medium: 3,
    high: 4,
    ultrahigh: 5,
};

const reliabilityViewEnum = {
    [reliabilityEnum.ultralow]: 'ultralow',
    [reliabilityEnum.low]: 'low',
    [reliabilityEnum.medium]: 'medium',
    [reliabilityEnum.high]: 'high',
    [reliabilityEnum.ultrahigh]: 'ultrahigh',
};

const getReliability = (pairExchange) => {
    if (Date.now() - pairExchange.last_update_ts < 70 && pairExchange.latency < 50) return reliabilityEnum.ultrahigh;
    if (Date.now() - pairExchange.last_update_ts < 120 && pairExchange.latency < 100) return reliabilityEnum.high;
    if (Date.now() - pairExchange.last_update_ts < 220 && pairExchange.latency < 200) return reliabilityEnum.medium;
    if (Date.now() - pairExchange.last_update_ts < 320 && pairExchange.latency < 300) return reliabilityEnum.low;

    return reliabilityEnum.ultralow;
}

const riskCoef = 4;

const toPairExchange = (binary_arr) => ({
    price: binary_arr[0],
    latency: binary_arr[1],
    last_update_ts: binary_arr[2],
})

const boxDecimal = (num) => Math.round(num * 100000);
const unboxDecimal = (num) => num / 100000;

websocket.onmessage = (event) => {
    package_count++;
    const bytes = new Uint8Array(event.data);
    const parsed = msgpack.decode(bytes);

    Object.entries(parsed).forEach(([pairName, pair]) => {
        if (pairName.endsWith("-perp")) return;

        const allExchangesMap = Object.entries(pair);
        const allPerpExchangesMap = Object.entries(parsed[`${pairName}-perp`] || {});

        if (!allPerpExchangesMap) return;

        allExchangesMap.forEach(([exchangeName, _pairExchange]) => {
            const pairExchange = toPairExchange(_pairExchange);
            allPerpExchangesMap.forEach(([otherExchangeName, _otherPairExchange]) => {
                const otherPairExchange = toPairExchange(_otherPairExchange);
                if (otherExchangeName !== exchangeName) {
                    const highest = Math.max(pairExchange.price, otherPairExchange.price);
                    const lowest = Math.min(pairExchange.price, otherPairExchange.price);
                    const diffPercent = ((boxDecimal(highest) - boxDecimal(lowest)) / boxDecimal(lowest)) * 100.0;
                    const acceptableThreshold = (arbitrageThresholds[pairName] || 0.5) / riskCoef;
                    if (diffPercent >= acceptableThreshold) {
                        const firstReliability = getReliability(pairExchange);
                        const secondReliability = getReliability(otherPairExchange);

                        const isHighReliability = firstReliability > reliabilityEnum.ultralow && secondReliability > reliabilityEnum.ultralow;

                        if (isHighReliability) {
                            const cheaperExchange = pairExchange.price < otherPairExchange.price ? exchangeName : otherExchangeName;
                            const expensiveExchange = pairExchange.price < otherPairExchange.price ? otherExchangeName : exchangeName;
                            // if (cheaperExchange === 'bybit' && expensiveExchange === 'binance') {
                            console.log(`Arbitrage opportunity (${pairName})`, `Buy on ${cheaperExchange} at ${Math.round(Math.min(pairExchange.price, otherPairExchange.price) * 100000) / 100000}`, `Sell on ${expensiveExchange} at ${Math.round(Math.max(pairExchange.price, otherPairExchange.price) * 100000) / 100000}`, 'Diff percent', '-', Math.round(diffPercent * 100) / 100, '%');
                            // }
                            // console.log(`Arbitrage opportunity (${pairName})`, `${exchangeName} (${pairExchange.price}) (${reliabilityViewEnum[firstReliability]})`, '-', `${otherExchangeName} (${otherPairExchange.price}) (${reliabilityViewEnum[secondReliability]})`, 'Diff percent', '-', Math.round(diffPercent * 100) / 100, '%');
                        }
                    }
                }
            }); return;

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