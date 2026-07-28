import("MrxGui")
import("MrxCheatBootstrap")
import("MrxFactionManager")
import("MrxSupportData")
import("WifStarterData")
import("MrxStarterManager")
import("WifEquipmentData")
import("MrxUtil")

function AddUnlockedItem(tItemData)
  if MrxCheatBootstrap.IsSkipModeEnabled() then
    return
  end
  local sMessage = _BuildMessage(tItemData.sType, tItemData)
  if sMessage then
    Hud.EventFanfare:Commence({
      sType = tItemData.sType,
      vText = sMessage
    })
  end
  if Net.IsServer() then
    if tItemData.sType == "outfit" then
      return
    end
    local uSupportHash = 0
    if tItemData.sSupportId then
      uSupportHash = String.GetHash(tItemData.sSupportId)
    end
    Net.SendEvent_UnlockFanfare(tItemData.sType, tItemData.sName, tItemData.sFactionId, MrxStarterManager.GetStarterIndexFromName(tItemData.sStarterId), uSupportHash, tItemData.nQty)
  end
end

function AddUnlockedItems(sType, tItems)
  if MrxCheatBootstrap.IsSkipModeEnabled() then
    return
  end
  local tMessages = {}
  for i, tItemData in ipairs(tItems) do
    local sMessage = _BuildMessage(sType, tItemData)
    if Net.IsServer() then
      local bClear = false
      if i == 1 then
        bClear = true
      end
      Net.SendEvent_BatchUnlockFanfare(false, sType, tItemData.sFactionId, tItemData.sSupportId, tItemData.nQty, bClear)
    end
    if sMessage then
      table.insert(tMessages, sMessage)
    end
  end
  if Net.IsServer() then
    Net.SendEvent_BatchUnlockFanfare(true)
  end
  if 0 < #tMessages then
    Hud.EventFanfare:Commence({sType = sType, vText = tMessages})
  end
end

function _BuildMessage(sType, tItemData)
  local sMessage, sFaction, sFactionIcon, sStarter, sSupport, sEquipment
  if tItemData.sFactionId and tItemData.sFactionId ~= "" then
    sFaction = MrxFactionManager.GetPlayerVisibleName(tItemData.sFactionId)
    sFactionIcon = MrxFactionManager.GetInlineIcon(tItemData.sFactionId)
  end
  if tItemData.sStarterId then
    sStarter = WifStarterData.GetPlayerVisibleName(tItemData.sStarterId)
  end
  if tItemData.sSupportId then
    sSupport = MrxSupportData.GetPlayerVisibleName(tItemData.sSupportId)
    sSupport = sSupport or tItemData.sSupportId .. " (INVALID SUPPORT ID?)"
  end
  if tItemData.sEquipmentId then
    sEquipment = WifEquipmentData.GetPlayerVisibleName(tItemData.sEquipmentId)
    sEquipment = sEquipment or tItemData.sEquipmentId .. " (INVALID EQUIPMENT ID?)"
  end
  if sType == "contact" then
    sMessage = ""
    if sFactionIcon then
      sMessage = sMessage .. sFactionIcon .. " "
    end
    sMessage = sMessage .. sStarter
  elseif sType == "support" then
    sMessage = ""
    if sSupport then
      sMessage = sMessage .. sFactionIcon .. " " .. sSupport
    elseif sEquipment then
      sMessage = sMessage .. sFactionIcon .. " " .. sEquipment
    end
  elseif sType == "stockpile" then
    sMessage = sSupport .. " (x " .. tItemData.nQty .. ")"
  elseif sType == "landingzone" then
    sMessage = ""
    if sFactionIcon then
      sMessage = sMessage .. sFactionIcon .. " "
    end
    if tItemData.sName then
      sMessage = sMessage .. tItemData.sName
    end
  elseif sType == "bounty" then
    sMessage = sFactionIcon .. " " .. sFaction
  elseif sType == "outfit" then
    sMessage = tItemData.sName
  end
  return sMessage
end

function SetClientFanfareData(sType, sName, sFactionId, nStarterId, sSupportId, nQty)
  sSupportId = MrxSupportData.GetSupportStringIndex(sSupportId)
  AddUnlockedItem({
    sType = sType,
    sName = sName,
    sFactionId = sFactionId,
    sStarterId = MrxStarterManager.GetStarterNameFromIndex(nStarterId),
    sSupportId = sSupportId,
    nQty = nQty
  })
end

tClientMessages = {}
_ClientBatchType = ""

function SetClientBatchFanfareData(bComplete, sType, sFactionId, sSupportId, nQty, bClear)
  if MrxCheatBootstrap.IsSkipModeEnabled() then
    return
  end
  if #tClientMessages > 0 and bComplete then
    Hud.EventFanfare:Commence({sType = _ClientBatchType, vText = tClientMessages})
    return
  end
  if bClear then
    tClientMessages = {}
    _ClientBatchType = ""
  end
  _ClientBatchType = sType
  local sMessage = _BuildMessage(sType, {
    sFactionId = sFactionId,
    sSupportId = MrxSupportData.GetSupportStringIndex(sSupportId),
    nQty = nQty
  })
  if sMessage then
    table.insert(tClientMessages, sMessage)
  end
end

function ClientHVTFanfare(iFanfareType, sFactionId, sDesc, iInlineIcon, nCompleted, nQuota)
  if not Net.IsClient() then
    return
  end
  if iFanfareType == 1 then
    sFanfareType = "hvtcapture"
  elseif iFanfareType == 2 then
    sFanfareType = "hvtkill"
  end
  local sDesc = MrxUtil.GetInlineIconNameByIndex(iInlineIcon) .. " " .. sDesc .. " " .. "(" .. nCompleted .. "/" .. nQuota .. ")"
  local sFanfareText = MrxFactionManager.GetInlineIcon(sFactionId) .. " " .. sDesc
  Debug.Printf(sFanfareText)
  Hud.EventFanfare:Commence({sType = sFanfareType, vText = sFanfareText})
end
