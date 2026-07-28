import("MrxGui")
import("MrxGuiBase")
import("MrxPmc")
import("MrxSupportData")
import("MrxGuiDialogBox")
import("MrxGuiManager")
sFlashFile = "garage"

function Create(uPlayerGuid)
  if "userdata" ~= type(uPlayerGuid) then
    return false
  end
  if _tGarageList[uPlayerGuid] then
    return false
  end
  local oFlash = MrxGui.FlashWidget:new()
  oFlash:SetOwner(uPlayerGuid)
  oFlash:SetVisible(false)
  oFlash:Pause()
  oFlash.CustomData.tItems = {}
  oFlash.AddItem = _AddItemWidget
  oFlash.SetCallback = _SetCallbackWidget
  oFlash.SetCloseCallback = _SetCloseCallbackWidget
  oFlash.Commence = _CommenceWidget
  if sFlashFile then
    oFlash:SetSwfFile(sFlashFile, _FlashLoadedCallback, {oFlash})
    MrxGui.AddWidget(oFlash)
  else
    oFlash.CustomData.bLoaded = true
  end
  local nHalfWidth = 283.33334
  oFlash:SetLocation(320 - nHalfWidth, 0, 320 + nHalfWidth, 480)
  _tGarageList[uPlayerGuid] = oFlash
  return true
end

function AddItem(uPlayerGuid, sId, sName, sDescription, sType, nCurrentStock, nMaxStock, sIcon, bNew)
  if "userdata" ~= type(uPlayerGuid) then
    return false
  end
  if not _tGarageList[uPlayerGuid] then
    return false
  end
  return _tGarageList[uPlayerGuid]:AddItem(sId, sName, sDescription, sType, nCurrentStock, nMaxStock, sIcon, bNew)
end

function _AddItemWidget(oFlash, sId, sName, sDescription, sType, nCurrentStock, nMaxStock, sIcon, bNew)
  if "string" ~= type(sName) or "string" ~= type(sDescription) then
    return false
  end
  if "number" ~= type(nCurrentStock) or "number" ~= type(nMaxStock) then
    return false
  end
  if "string" == type(sType) then
    sType = string.lower(sType)
  end
  local tData = {
    sId = sId,
    sName = sName,
    sDescription = sDescription,
    sType = sType,
    nCurrentStock = nCurrentStock,
    nMaxStock = nMaxStock,
    sIcon = sIcon,
    bNew = bNew
  }
  table.insert(oFlash.CustomData.tItems, tData)
  return true
end

function SetCallback(uPlayerGuid, fCallback, tCallbackData)
  if "userdata" ~= type(uPlayerGuid) then
    return false
  end
  if not _tGarageList[uPlayerGuid] then
    return false
  end
  return _tGarageList[uPlayerGuid]:SetCallback(fCallback, tCallbackData)
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

function Commence(uPlayerGuid)
  if "userdata" ~= type(uPlayerGuid) then
    return
  end
  if not _tGarageList[uPlayerGuid] then
    return
  end
  return _tGarageList[uPlayerGuid]:Commence()
end

function _CommenceWidget(oFlash)
  oFlash.CustomData.bRunning = true
  if oFlash.CustomData.bLoaded then
    _RunGarage(oFlash)
  end
end

_tGarageList = false

function Init()
  _tGarageList = {}
end

function _FlashLoadedCallback(oFlashWidget)
  oFlashWidget.CustomData.bLoaded = true
  oFlashWidget:Pause()
  if oFlashWidget.CustomData.bRunning then
    _RunGarage(oFlashWidget)
  end
end

function _RunGarage(oFlashWidget)
  if sFlashFile then
    _SetupGarageFlash(oFlashWidget)
    oFlashWidget:SetVisible(true)
    MrxGuiBase.GetControlFocus(oFlashWidget, true)
    oFlashWidget:Play()
    if MrxGuiManager.GetHudState() then
      MrxGuiManager.ToggleHud(oFlashWidget:GetOwner(), false)
      oFlashWidget.CustomData.bRestoreHud = true
    end
  end
end

function _SetupGarageFlash(oFlash)
  oFlash:CallActionScriptCallback("AddStockpile", {
    MrxPmc.GetCashQty(),
    MrxPmc.GetFuelQty(),
    MrxPmc.GetFuelCapacity()
  })
  local sFunctionName
  for nIndex, tData in pairs(oFlash.CustomData.tItems) do
    if "light" == tData.sType then
      sFunctionName = "AddSupportLight"
    elseif "heavy" == tData.sType then
      sFunctionName = "AddSupportHeavy"
    elseif "helicopters" == tData.sType then
      sFunctionName = "AddSupportHelicopters"
    elseif "boats" == tData.sType then
      sFunctionName = "AddSupportBoats"
    else
      sFunctionName = "AddSupportCivilian"
    end
    oFlash:CallActionScriptCallback(sFunctionName, {
      tData.sId,
      tData.sName,
      tData.sDescription,
      tData.sIcon,
      tData.nCurrentStock,
      tData.nMaxStock,
      0,
      tData.bNew or false
    })
  end
  oFlash:SetFlashEventHandler("vehicleSelect", _EndCallback, {})
  oFlash:SetFlashEventHandler("closePDA", _CloseCallback)
  local oBg = MrxGui.ImageWidget:new()
  oBg:SetFullscreen(true)
  oBg:SetColor(0, 0, 0, 192)
  oBg:SetOwner(oFlash:GetOwner())
  oFlash.CustomData.oBg = oBg
  MrxGui.RemoveWidget(oFlash)
  MrxGui.AddWidget(oBg)
  MrxGui.AddWidget(oFlash)
end

function _EndCallback(oFlash, sArg)
  local uPlayer = oFlash:GetOwner()
  _tGarageList[uPlayer] = nil
  MrxGuiBase.ReleaseControlFocus(oFlash)
  Event.Create(Event.TimerRelative, {0.1, true}, _RemoveFlashFile, {oFlash})
  if oFlash.CustomData.fCallback then
    local tCallbackData = {}
    if oFlash.CustomData.tCallbackData then
      for n, v in pairs(oFlash.CustomData.tCallbackData) do
        tCallbackData[n] = v
      end
    end
    if sArg then
      table.insert(tCallbackData, sArg)
    end
    oFlash.CustomData.fCallback(unpack(tCallbackData))
  end
end

function _CloseCallback(oFlash)
  _EndCallback(oFlash, nil)
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
