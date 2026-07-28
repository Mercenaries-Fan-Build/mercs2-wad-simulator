import("MrxGui")
import("MrxGuiBase")
import("MrxPmc")
import("MrxSupportData")
import("MrxGuiDialogBox")
import("MrxGuiManager")
sFlashFile = "store"

function Create(uPlayerGuid)
  if "userdata" ~= type(uPlayerGuid) then
    return false
  end
  if _tShopList[uPlayerGuid] then
    return false
  end
  local oFlash = MrxGui.FlashWidget:new()
  oFlash:SetOwner(uPlayerGuid)
  oFlash:SetVisible(false)
  oFlash:Pause()
  oFlash.CustomData.tItems = {}
  oFlash.AddItem = _AddItemWidget
  oFlash.AddItemFull = _AddItemFullWidget
  oFlash.SetCallback = _SetCallbackWidget
  oFlash.SetCloseCallback = _SetCloseCallbackWidget
  oFlash.Commence = _CommenceWidget
  if sFlashFile then
    oFlash:SetSwfFile(sFlashFile, _FlashLoadedCallback, {oFlash})
    MrxGui.AddWidget(oFlash)
  else
    oFlash.CustomData.bLoaded = true
  end
  oFlash:SetAnchoring("center", "center")
  oFlash:SetFullscreen(true)
  _tShopList[uPlayerGuid] = oFlash
  return true
end

function AddItem(uPlayerGuid, sName, nCashCost, nCurrentStock, nMaxStock, bUnlocked, sId, bFuelTank, nFuelQuantity, sRawName)
  if "userdata" ~= type(uPlayerGuid) then
    return false
  end
  if not _tShopList[uPlayerGuid] then
    return false
  end
  return _tShopList[uPlayerGuid]:AddItem(sName, nCashCost, nCurrentStock, nMaxStock, bUnlocked, sId, bFuelTank, nFuelQuantity, sRawName)
end

function _AddItemWidget(oFlash, sName, nCashCost, nCurrentStock, nMaxStock, bUnlocked, sId, bFuelTank, nFuelQuantity, sRawName)
  if "string" ~= type(sName) then
    return false
  end
  if "number" ~= type(nCashCost) or "number" ~= type(nCurrentStock) or "number" ~= type(nMaxStock) then
    return false
  end
  local tData = {
    sName = sName,
    nCashCost = nCashCost,
    nCurrentStock = nCurrentStock,
    nMaxStock = nMaxStock,
    bUnlocked = bUnlocked,
    sId = sId,
    bFuelTank = bFuelTank,
    nFuelQuantity = nFuelQuantity,
    sRawName = sRawName
  }
  table.insert(oFlash.CustomData.tItems, tData)
  return true
end

function AddItemFull(uPlayerGuid, sName, sDesc, sTexture, nCashCost, nCurrentStock, nMaxStock, bUnlocked, sId, bFuelTank, bMarkAsNew, nFuelQuantity, sRawName)
  if "userdata" ~= type(uPlayerGuid) then
    return false
  end
  if not _tShopList[uPlayerGuid] then
    return false
  end
  return _tShopList[uPlayerGuid]:AddItemFull(sName, sDesc, sTexture, nCashCost, nCurrentStock, nMaxStock, bUnlocked, sId, bFuelTank, bMarkAsNew, nFuelQuantity, sRawName)
end

function _AddItemFullWidget(oFlash, sName, sDesc, sTexture, nCashCost, nCurrentStock, nMaxStock, bUnlocked, sId, bFuelTank, bMarkAsNew, nFuelQuantity, sRawName)
  if "string" ~= type(sName) or "string" ~= type(sDesc) or "string" ~= type(sTexture) or nil == sId then
    return false
  end
  if "number" ~= type(nCashCost) or "number" ~= type(nCurrentStock) or "number" ~= type(nMaxStock) then
    return false
  end
  local tData = {
    sName = sName,
    sDesc = sDesc,
    sTexture = sTexture,
    nCashCost = nCashCost,
    nCurrentStock = nCurrentStock,
    nMaxStock = nMaxStock,
    bUnlocked = bUnlocked,
    sId = sId,
    bFuelTank = bFuelTank,
    bMarkAsNew = bMarkAsNew,
    nFuelQuantity = nFuelQuantity,
    sRawName = sRawName
  }
  table.insert(oFlash.CustomData.tItems, tData)
  return true
end

function SetCallback(uPlayerGuid, fCallback, tCallbackData)
  if "userdata" ~= type(uPlayerGuid) then
    return false
  end
  if not _tShopList[uPlayerGuid] then
    return false
  end
  return _tShopList[uPlayerGuid]:SetCallback(fCallback, tCallbackData)
end

function _SetCallbackWidget(oFlash, fCallback, tCallbackData)
  if nil == fCallback then
    oFlash.CustomData.fCallback = nil
    oFlash.CustomData.tCallbackData = nil
    return true
  end
  if "function" ~= type(fCallback) then
    return false
  end
  oFlash.CustomData.fCallback = fCallback
  if "table" == type(tCallbackData) then
    oFlash.CustomData.tCallbackData = tCallbackData
  else
    oFlash.CustomData.tCallbackData = nil
  end
  return true
end

function SetCloseCallback(uPlayerGuid, fCallback, tCallbackData)
  if "userdata" ~= type(uPlayerGuid) then
    return false
  end
  if not _tShopList[uPlayerGuid] then
    return false
  end
  return _tShopList[uPlayerGuid]:SetCloseCallback(fCallback, tCallbackData)
end

function _SetCloseCallbackWidget(oFlash, fCallback, tCallbackData)
  if nil == fCallback then
    oFlash.CustomData.fCloseCallback = nil
    oFlash.CustomData.tCloseCallbackData = nil
    return true
  end
  if "function" ~= type(fCallback) then
    return false
  end
  oFlash.CustomData.fCloseCallback = fCallback
  if "table" == type(tCallbackData) then
    oFlash.CustomData.tCloseCallbackData = tCallbackData
  else
    oFlash.CustomData.tCloseCallbackData = nil
  end
  return true
end

function Commence(uPlayerGuid)
  if "userdata" ~= type(uPlayerGuid) then
    return
  end
  if not _tShopList[uPlayerGuid] then
    return
  end
  return _tShopList[uPlayerGuid]:Commence()
end

function _CommenceWidget(oFlash)
  oFlash.CustomData.bRunning = true
  if oFlash.CustomData.bLoaded then
    _RunShop(oFlash)
  end
end

function Close(uPlayerGuid)
  if "userdata" ~= type(uPlayerGuid) then
    Debug.Printf("uPlayerGuid not of type userdata")
    return
  end
  if not _tShopList[uPlayerGuid] then
    Debug.Printf("Shop interface not found for player guid: " .. tostring(uPlayerGuid))
    return
  end
  _FlashCloseShopCallback(_tShopList[uPlayerGuid], nil)
end

_tShopList = false

function Init()
  _tShopList = {}
end

function _FlashLoadedCallback(oFlashWidget)
  oFlashWidget.CustomData.bLoaded = true
  oFlashWidget:Pause()
  if oFlashWidget.CustomData.bRunning then
    _RunShop(oFlashWidget)
  end
end

function _RunShop(oFlashWidget)
  if sFlashFile then
    _SetupShopFlash(oFlashWidget)
    oFlashWidget:SetVisible(true)
    MrxGuiBase.GetControlFocus(oFlashWidget, true)
    oFlashWidget:Play()
    if MrxGuiManager.GetHudState() then
      MrxGuiManager.ToggleHud(oFlashWidget:GetOwner(), false)
      oFlashWidget.CustomData.bRestoreHud = true
    end
  else
    _CreateShopDialogBox(oFlashWidget)
  end
  LTILibName.ChangeShellState(true)
end

function _CreateShopDialogBox(oFlash)
  local tOptions = {}
  for nIndex, tData in pairs(oFlash.CustomData.tItems) do
    local sColor
    if not tData.bUnlocked then
      sColor = ""
    elseif tData.nCashCost > MrxPmc.GetCashQty() then
      sColor = "[red]"
    else
      sColor = "[green]"
    end
    tOptions[nIndex] = string.format("%s%s ($%d) (%d/%d)", sColor, tData.sName, tData.nCashCost, tData.nCurrentStock, tData.nMaxStock)
  end
  local nCancelIndex = #tOptions + 1
  tOptions[nCancelIndex] = "[Generic.Cancel]"
  oFlash.CustomData.nCancelIndex = nCancelIndex
  MrxGuiDialogBox.DisplayDialogBox(oFlash:GetOwner(), "[Generic.Cash]: $" .. MrxPmc.GetCashQty(), tOptions, 1, _CloseShopDialogBox, {oFlash})
end

function _CloseShopDialogBox(oFlash, nSelectedIndex)
  if nSelectedIndex ~= oFlash.CustomData.nCancelIndex and oFlash.CustomData.fCallback then
    local tData = oFlash.CustomData.tCallbackData or {}
    table.insert(tData, oFlash.CustomData.tItems[nSelectedIndex].sName)
    table.insert(tData, 1)
    oFlash.CustomData.fCallback(unpack(tData))
  end
  local uPlayer = oFlash:GetOwner()
  oFlash:delete()
  _tShopList[uPlayer] = nil
end

function _SetupShopFlash(oFlash)
  oFlash:CallActionScriptCallback("AddStockpile", {
    MrxPmc.GetCashQty(),
    MrxPmc.GetFuelQty(),
    MrxPmc.GetFuelCapacity()
  })
  local tEquippedSupport = {}
  local nSlots = 3
  local oPda = MrxGui.GetWidgetByNameAndOwner("PDA", oFlash:GetOwner())
  if oPda then
    local n = 1
    local sName, sIcon
    while nSlots >= n do
      sName, sIcon = oPda:GetEquippedSupport(n)
      tEquippedSupport[n] = {
        sName = sName,
        sIcon = sIcon,
        nId = #oFlash.CustomData.tItems + 1,
        sRawName = sName
      }
      table.insert(oFlash.CustomData.tItems, tEquippedSupport[n])
      n = n + 1
    end
  end
  for nIndex, tData in pairs(oFlash.CustomData.tItems) do
    local tSupportData = MrxSupportData.tSupportData[tData.sId]
    local bEquippable = MrxSupportData.IsSupportEquippable(tData.sId)
    local sDesignatorDisplay = " "
    if tSupportData and tSupportData.oSupport and tSupportData.oSupport:GetDesignator() then
      local sDesignator = tSupportData.oSupport:GetDesignator():GetType()
      if "smoke" == sDesignator then
        sDesignatorDisplay = "[Generic.SupportDesignators.Smoke]"
      elseif "satellite" == sDesignator then
        sDesignatorDisplay = "[Generic.SupportDesignators.Satellite]"
      elseif "advanced satellite" == sDesignator then
        sDesignatorDisplay = "[Generic.SupportDesignators.AdvSatellite]"
      elseif "beacon" == sDesignator then
        sDesignatorDisplay = "[Generic.SupportDesignators.Beacon]"
      elseif "laser" == sDesignator then
        sDesignatorDisplay = "[Generic.SupportDesignators.Laser]"
      elseif "flare" == sDesignator then
        sDesignatorDisplay = "[Generic.SupportDesignators.Flare]"
      end
    end
    if tData.sDesc and tData.sTexture and tData.nCurrentStock then
      oFlash:CallActionScriptCallback("AddShopItem", {
        nIndex,
        tData.sName,
        tData.sDesc,
        tData.sTexture,
        tData.nCurrentStock or 0,
        tData.nMaxStock,
        tData.nCashCost,
        tData.bUnlocked or false,
        tData.bFuelTank or false,
        tData.bMarkAsNew or false,
        bEquippable,
        tData.nFuelQuantity or 9999,
        sDesignatorDisplay
      })
    elseif tSupportData then
      oFlash:CallActionScriptCallback("AddShopItem", {
        nIndex,
        tData.sName,
        tSupportData.sDescription,
        tSupportData.sIcon,
        MrxPmc.GetSupportQty(tData.sId) or 0,
        tSupportData.nMaxStock,
        tData.nCashCost or tSupportData.nCashCost,
        tSupportData.bUnlocked or false,
        tData.bFuelTank or false,
        tData.bMarkAsNew or false,
        bEquippable,
        tData.nFuelQuantity or 9999,
        sDesignatorDisplay
      })
    end
    for n, tEquippedData in pairs(tEquippedSupport) do
      if tData.sRawName == tEquippedData.sRawName then
        tEquippedData.sName = tData.sName
      end
    end
  end
  for nSlot, tData in pairs(tEquippedSupport) do
    if tData.nId and tData.sName then
      oFlash:CallActionScriptCallback("AddSupportEquipped", {
        nSlot,
        tData.nId,
        tData.sName,
        tData.sIcon
      })
    end
  end
  oFlash:SetFlashEventHandler("BuyStockpile", _FlashSupportBoughtCallback, {})
  oFlash:SetFlashEventHandler("equip", _FlashSupportEquippedCallback, {})
  oFlash:SetFlashEventHandler("closeStore", _FlashCloseShopCallback, {})
  local oBg = MrxGui.ImageWidget:new()
  oBg:SetFullscreen(true)
  oBg:SetColor(0, 0, 0, 192)
  oBg:SetOwner(oFlash:GetOwner())
  oFlash.CustomData.oBg = oBg
  MrxGui.RemoveWidget(oFlash)
  MrxGui.AddWidget(oBg)
  MrxGui.AddWidget(oFlash)
end

function _FlashSupportBoughtCallback(oFlash, sArg)
  local t = {}
  local nIndex = 1
  for nNumber in string.gmatch(sArg, "-*%d+") do
    t[nIndex] = tonumber(nNumber)
    nIndex = nIndex + 1
  end
  if t[1] and t[2] then
    local tData = oFlash.CustomData.tItems[t[1]]
    if tData and oFlash.CustomData.fCallback then
      local tCallbackData = {}
      if oFlash.CustomData.tCallbackData then
        for n, v in pairs(oFlash.CustomData.tCallbackData) do
          tCallbackData[n] = v
        end
      end
      table.insert(tCallbackData, tData.sId)
      table.insert(tCallbackData, t[2])
      oFlash.CustomData.fCallback(unpack(tCallbackData))
    end
  end
end

function _FlashSupportEquippedCallback(oFlash, sData)
  local nSlot, nIndex = _ParseString(sData)
  if nIndex and nSlot and oFlash.CustomData.tItems[nIndex] then
    local sName = oFlash.CustomData.tItems[nIndex].sRawName
    local oPda = MrxGui.GetWidgetByNameAndOwner("PDA", oFlash:GetOwner())
    if oPda and sName then
      oPda:SetEquippedSupport(sName, nSlot)
    end
  end
end

function _ParseString(sData)
  local nSlot, nId
  for nNumber, sSeperator, nIdentifier in string.gmatch(sData, "(%d+)([, ]*)(%w+)") do
    nSlot = tonumber(nNumber)
    nId = tonumber(nIdentifier)
  end
  if "number" == type(nSlot) and "number" == type(nId) then
    return nSlot, nId
  end
  return nil
end

function _FlashCloseShopCallback(oFlash, sArg)
  local fCallback = oFlash.CustomData.fCloseCallback
  local tCallbackData = oFlash.CustomData.tCloseCallbackData
  local uPlayer = oFlash:GetOwner()
  _tShopList[uPlayer] = nil
  MrxGuiBase.ReleaseControlFocus(oFlash)
  if fCallback then
    tCallbackData = tCallbackData or {}
    fCallback(unpack(tCallbackData))
  end
  oFlash:CallActionScriptCallback("requestClose", {})
  _RemoveFlashFile(oFlash)
end

function _RemoveFlashFile(oFlash)
  if oFlash.CustomData.bRestoreHud then
    MrxGuiManager.ToggleHud(oFlash:GetOwner(), true)
  end
  local oBg = oFlash.CustomData.oBg
  oFlash:SetSwfFile(nil)
  MrxGui.RemoveWidget(oFlash)
  oFlash:delete()
  MrxGui.RemoveWidget(oBg)
  oBg:delete()
end
