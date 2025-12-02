{
  # Optional: Add balances not tracked on exchanges (in USD)
  # other_balances = 1000.0;
  exchanges =
    let
      binance_tiger = "BINANCE_TIGER_FULL"; #dbg: just testing out to see if this is valid
    in
      {
      binance = {
        api_pubkey.env = "${binance_tiger}_PUBKEY";
        api_secret.env = "${binance_tiger}_SECRET";
      };
      bybit = {
        api_pubkey.env = "QUANTM_BYBIT_SUB_PUBKEY";
        api_secret.env = "QUANTM_BYBIT_SUB_SECRET";
      };
      mexc = {
        api_pubkey.env = "MEXC_READ_PUBKEY";
        api_secret.env = "MEXC_READ_SECRET";
      };
      kucoin = {
        api_pubkey.env = "KUCOIN_API_PUBKEY";
        api_secret.env = "KUCOIN_API_SECRET";
        passphrase.env = "KUCOIN_API_PASSPHRASE";
      };
    };
  size = {
    default_sl = 0.02;
    round_bias = "5%";

    risk_tiers = {
      a = "20%";
      b = "8%";
      c = "3%";
      d = "1%";
      e = "0.25%";
    };
  };
}
