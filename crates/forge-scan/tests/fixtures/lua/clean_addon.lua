-- A typical, harmless addon: a frame, events, saved variables and chat output.
LuckyDB = LuckyDB or { shown = true, scale = 1.0 }

local Lucky = CreateFrame("Frame", "LuckyFrame", UIParent)
Lucky:SetWidth(200)
Lucky:SetHeight(40)
Lucky:SetPoint("CENTER", UIParent, "CENTER", 0, 0)
Lucky:RegisterEvent("PLAYER_LOGIN")
Lucky:RegisterEvent("PLAYER_LEVEL_UP")

local text = Lucky:CreateFontString(nil, "OVERLAY", "GameFontNormal")
text:SetPoint("CENTER", Lucky, "CENTER", 0, 0)

local function percent(value, total)
    if total == 0 then
        return 0
    end
    return math.floor(value / total * 100 + 0.5)
end

Lucky:SetScript("OnEvent", function()
    if event == "PLAYER_LOGIN" then
        Lucky:SetScale(LuckyDB.scale)
        DEFAULT_CHAT_FRAME:AddMessage("|cff33ff99Lucky|r loaded. Type /lucky for options.")
    elseif event == "PLAYER_LEVEL_UP" then
        text:SetText(string.format("Level %d: %d%% luck", arg1, percent(arg1, 60)))
    end
end)

SLASH_LUCKY1 = "/lucky"
SlashCmdList["LUCKY"] = function(msg)
    local cmd = string.lower(msg or "")
    if cmd == "hide" then
        LuckyDB.shown = false
        Lucky:Hide()
    else
        LuckyDB.shown = true
        Lucky:Show()
    end
end

local frame = getglobal("LuckyFrame")
local options = { os = "unused key", io = 2 }
local t = {}
table.insert(t, "x")
