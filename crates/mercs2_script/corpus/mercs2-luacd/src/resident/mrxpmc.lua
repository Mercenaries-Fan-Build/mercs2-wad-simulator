import("WifEquipmentData")
import("MrxGui")
import("MrxSupportData")
import("MrxUnlockFanfare")
import("MrxUtil")
import("MrxStatsManager")
import("MrxTutorialManager")
import("WifMissionFlow")
local _ksFuelTank = "_pmcoutpost_bld_fueldepot"
local _ksFuelTankLocation = "PMC Fuel Tank "
local _tFactionToAssociation = {
  All = "Allied",
  Chi = "China",
  Civ = "Civilian",
  Gur = "Guerilla",
  Oil = "OC",
  Pir = "Pirate",
  Pmc = "PMC",
  Vza = "VZ"
}
local _tPluralReasonToSingular = {
  ["[Generic.Collectibles]"] = "[Generic.Collectible]",
  ["[Generic.Wagers]"] = "[Generic.Wager]",
  ["[Generic.Contracts]"] = "[Generic.Contract]",
  ["[Generic.Pickups]"] = "[Generic.Pickup]",
  ["[Generic.ShopItems]"] = "[Generic.ShopItem]",
  ["[Generic.Bribes]"] = "[Generic.Bribe]",
  ["[Generic.Medevacs]"] = "[Generic.Medevac]"
}
_tEvents = {}
_tStockpile = {}
_tFreebies = {}
_tEquipment = {}
_tStockpileThresholdInfo = {}
_nLatestCount = 0
_tClientSupportSpendings = {}
local _knMinFuelCapacity = 300
local _knMaxFuelCapacity = 9999

function Init()
  SetFuelCapacity(_knMinFuelCapacity, false, true)
  _CreateHiScoreEvent()
end

function AddCashQty(nAmt, bMateriel, sReason, bSuppressDisplay)
  local knBillion = 1000000000
  if knBillion < math.abs(nAmt) then
    if 0 < nAmt then
      nAmt = knBillion
    elseif nAmt < 0 then
      nAmt = -knBillion
    end
  end
  local nTotal = GetCashQty() + nAmt
  if knBillion < nTotal then
    nTotal = knBillion
  elseif nTotal < 0 then
    nTotal = 0
  end
  Player.SetCash(nTotal)
  if type(bSuppressDisplay) ~= "boolean" then
    bSuppressDisplay = false
  end
  if not bSuppressDisplay then
    DisplayCash(Player.GetCash(), sReason, nAmt)
  end
  Event.Post("CashAdded", {nAmtAdded = nAmt})
  if 0 < nAmt then
    MrxStatsManager.IncreaseCreditAmount(nAmt)
    if sReason then
      MrxStatsManager.ReasonsForCredits(sReason, nAmt)
    end
  elseif nAmt < 0 then
    MrxStatsManager.IncreaseDebitAmount(nAmt)
    if sReason then
      MrxStatsManager.ReasonsForDebits(sReason, nAmt)
    end
  end
end

function GetCashQty()
  return Player.GetCash()
end

function AddFuelQty(nAmt)
  local nNewAmt = GetFuelQty() + nAmt
  if nNewAmt <= GetFuelCapacity() and 0 <= nNewAmt then
    Player.AddFuel(nAmt)
  elseif nNewAmt > _nFuelCapacity then
    Player.SetFuel(_nFuelCapacity)
  elseif nNewAmt < 0 then
    Player.SetFuel(0)
  end
  if nAmt < 0 and nNewAmt <= 0 then
    MrxTutorialManager.StartTutorial("NoFuel")
  elseif nAmt < 0 and nNewAmt / _nFuelCapacity <= 0.1 then
    MrxTutorialManager.StartTutorial("LowFuel")
  end
  DisplayResources(nil, nAmt)
  if 0 < nAmt then
    MrxStatsManager.IncreaseFuelInAmount(nAmt)
  else
    MrxStatsManager.IncreaseFuelOutAmount(nAmt)
  end
end

function GetFuelQty()
  return Player.GetFuel()
end

function SetFuelCapacity(nFuelCapacity, bCheat, bDoNotSyncFuel)
  if bCheat or nFuelCapacity >= _knMinFuelCapacity and nFuelCapacity <= _knMaxFuelCapacity then
    _nFuelCapacity = nFuelCapacity
    if Player.GetFuel() > _nFuelCapacity and not bDoNotSyncFuel then
      Player.SetFuel(_nFuelCapacity)
    end
    return true
  end
  return false
end

function AddFuelCapacity(nFuelCapacity)
  local bResult = SetFuelCapacity(GetFuelCapacity() + nFuelCapacity)
  if not bResult then
    local n
    if 0 < nFuelCapacity then
      n = _knMaxFuelCapacity
    elseif nFuelCapacity < 0 then
      n = _knMinFuelCapacity
    end
    SetFuelCapacity(n)
  end
  DisplayResources()
end

function GetFuelCapacity()
  return _nFuelCapacity
end

function AddSupportQty(sName, nAmt, bDspFanfare, nCost)
  if not sName then
    return
  end
  Debug.Printf("AddSupportQty(): " .. tostring(nAmt) .. " of " .. tostring(sName) .. " for $" .. tostring(nCost))
  if _tStockpile[sName] then
    _tStockpile[sName].nAmt = _tStockpile[sName].nAmt + nAmt
  else
    _tStockpile[sName] = {nAmt = nAmt, bNew = true}
  end
  if Net.IsClient() and nCost then
    if not _tClientSupportSpendings[sName] then
      _tClientSupportSpendings[sName] = {nUnits = 0, nSpent = 0}
    end
    Debug.Printf("- [" .. sName .. "]: nUnits = " .. tostring(_tClientSupportSpendings[sName].nUnits) .. ", TotalSpent = " .. tostring(_tClientSupportSpendings[sName].nSpent))
    _tClientSupportSpendings[sName].nUnits = _tClientSupportSpendings[sName].nUnits + nAmt
    _tClientSupportSpendings[sName].nSpent = _tClientSupportSpendings[sName].nSpent + nCost
  end
  if Net.IsServer() and bDspFanfare then
    MrxUnlockFanfare.AddUnlockedItem({
      sType = "stockpile",
      sSupportId = sName,
      nQty = nAmt
    })
  end
  CheckSupportThreshold(sName)
  WifMissionFlow.RefreshAllPdaMissionDetails()
end

function SetSupportQty(sName, nAmt)
  if not sName then
    return
  end
  _tStockpile[sName] = _tStockpile[sName] or {}
  _tStockpile[sName].nAmt = nAmt
  CheckSupportThreshold(sName)
end

function GetSupportQty(sName)
  if sName and _tStockpile[sName] then
    return _tStockpile[sName].nAmt
  end
end

function SetSupportNew(sName, bNew)
  local tStockpileItemData = _tStockpile[sName]
  if tStockpileItemData then
    tStockpileItemData.bNew = bNew
  end
end

function SetAllSupportViewed()
  for sName, tData in pairs(_tStockpile) do
    tData.bNew = false
  end
end

function IsSupportNew(sName)
  local tStockpileItemData = _tStockpile[sName]
  if tStockpileItemData then
    return tStockpileItemData.bNew
  end
end

function CheckSupportThreshold(sName)
  if not _tStockpileThresholdInfo then
    return
  end
  local bDidCallback = false
  for i, tCallbackInfo in pairs(_tStockpileThresholdInfo) do
    local sSupportId = tCallbackInfo[2]
    if sSupportId == sName then
      local sComparisonOperator = tCallbackInfo[3]
      local nThreshold = tCallbackInfo[4]
      local nStockpileAmount = 0
      if _tStockpile[sName] then
        nStockpileAmount = _tStockpile[sName].nAmt
      end
      if sComparisonOperator == "<" and nThreshold > nStockpileAmount or sComparisonOperator == "<=" and nThreshold >= nStockpileAmount or sComparisonOperator == ">" and nThreshold < nStockpileAmount or sComparisonOperator == ">=" and nThreshold <= nStockpileAmount or sComparisonOperator == "==" and nStockpileAmount == nThreshold then
        if tCallbackInfo[5] == nil then
          Debug.Printf("callback is nill!!")
        end
        MrxUtil.CallWithOptionalArgs(tCallbackInfo[5], tCallbackInfo[6])
        tCallbackInfo.bDone = "done"
        bDidCallback = true
      end
    end
  end
  if bDidCallback == true then
    local i = #_tStockpileThresholdInfo
    while 0 < i do
      if _tStockpileThresholdInfo[i].bDone == "done" then
        table.remove(_tStockpileThresholdInfo, i)
      end
      i = i - 1
    end
  end
end

function SetStockpileChangeCallback(sName, sComparison, nThreshold, fCallback, tCallbackArgs)
  _nLatestCount = _nLatestCount + 1
  if _tStockpileThresholdInfo then
    table.insert(_tStockpileThresholdInfo, {
      _nLatestCount,
      sName,
      sComparison,
      nThreshold,
      fCallback,
      tCallbackArgs
    })
  else
    _tStockpileThresholdInfo = {
      {
        _nLatestCount,
        sName,
        sComparison,
        nThreshold,
        fCallback,
        tCallbackArgs
      }
    }
  end
  return _nLatestCount
end

function DeleteStockpileChangeCallback(nId)
  for i, tCallbackInfo in pairs(_tStockpileThresholdInfo) do
    if tCallbackInfo[1] == nId then
      table.remove(_tStockpileThresholdInfo, i)
      return
    end
  end
end

function AddFreebieQty(sName, nAmt)
  if not sName then
    return
  end
  if _tFreebies[sName] then
    _tFreebies[sName] = _tFreebies[sName] + nAmt
  else
    _tFreebies[sName] = nAmt
  end
end

function SetFreebieQty(sName, nAmt)
  if not sName then
    return
  end
  _tFreebies[sName] = nAmt
end

function GetFreebieQty(sName)
  return sName and _tFreebies[sName]
end

function AddEquipment(sName, bDoNotAddCapacity)
  local tEquipmentData = WifEquipmentData.GetEquipmentData(sName)
  if not tEquipmentData then
    return
  end
  if HasEquipment(sName) then
    return
  end
  if tEquipmentData.nType == WifEquipmentData.knTypeFuelTank then
    if bDoNotAddCapacity then
      AddFuelTank(sName, false)
    else
      AddFuelTank(sName, true)
    end
    DisplayResources()
  elseif tEquipmentData.nType == WifEquipmentData.knTypeGrapplingHook then
    WifMissionFlow.SetGrappleEnabled(true)
    _tEquipment[sName] = true
  else
    _tEquipment[sName] = true
  end
end

function RemoveEquipment(sName)
  local tEquipmentData = WifEquipmentData.GetEquipmentData(sName)
  if not tEquipmentData or not _tEquipment[sName] then
    return
  end
  if tEquipmentData.nType == WifEquipmentData.knTypeFuelTank then
    RemoveFuelTank(sName)
    DisplayResources()
  else
    _tEquipment[sName] = nil
  end
end

function HasEquipment(sName)
  local tEquipmentData = WifEquipmentData.GetEquipmentData(sName)
  if not tEquipmentData then
    return false
  end
  local tEquipment = _tEquipment[sName]
  if not tEquipment then
    return false
  end
  if tEquipmentData.nType == WifEquipmentData.knTypeFuelTank then
    return tEquipment.bPristine
  end
  return true
end

function DisplayCash(nCash, sReason, nChange)
  local sMappedReason = MapPluralReasonToSingular(sReason)
  sMappedReason = sMappedReason or sReason
  Hud.ResourceCounter:SetCash({
    nValue = nCash or 0,
    sReason = sMappedReason,
    nIncrement = nChange
  })
end

function MapPluralReasonToSingular(sReason)
  if sReason then
    return _tPluralReasonToSingular[sReason]
  end
end

function DisplayResources(nCashChange, nFuelChange)
  Hud.ResourceCounter:SetCash({
    nValue = Player.GetCash() or 0,
    nIncrement = nCashChange
  })
  Hud.ResourceCounter:SetFuel({
    nValue = Player.GetFuel() or 0,
    nMax = GetFuelCapacity(),
    nIncrement = nFuelChange
  })
end

function AddFuelTank(sName, bModifyCapacity)
  local tEquipmentData = WifEquipmentData.GetEquipmentData(sName)
  if not tEquipmentData then
    return
  end
  if bModifyCapacity then
    AddFuelCapacity(tEquipmentData.nFuelCapacity)
  end
  local tEquipment = _tEquipment[sName] or {}
  if tEquipment.uGuid then
    if tEquipment.bPristine then
      return
    else
      Object.Remove(tEquipment.uGuid)
    end
  end
  local uTank = MrxUtil.SpawnObject(_ksFuelTank, _ksFuelTankLocation .. tEquipmentData.nFuelTankId)
  Debug.Printf("Spawned " .. _ksFuelTank .. " at " .. _ksFuelTankLocation .. tEquipmentData.nFuelTankId .. " (" .. tostring(uTank) .. ")")
  tEquipment.uGuid = uTank
  tEquipment.bPristine = true
  _tEquipment[sName] = tEquipment
  if not _nFuelTanks then
    _uFuelTankDeath = Event.CreatePersistent(Event.ObjectDeath, {"FuelTank"}, _OnFuelTankDeath)
    _nFuelTanks = 0
  end
  _nFuelTanks = _nFuelTanks + 1
end

function RemoveFuelTank(sName)
  local tEquipmentData = WifEquipmentData.GetEquipmentData(sName)
  if not tEquipmentData then
    return
  end
  AddFuelCapacity(-tEquipmentData.nFuelCapacity)
  Debug.Printf("FuelTank " .. tEquipmentData.nFuelTankId .. " removed!")
  if _tEquipment[sName] then
    Object.Remove(_tEquipment.uGuid)
  end
  _tEquipment[sName] = nil
  _nFuelTanks = _nFuelTanks - 1
  if _nFuelTanks == 0 then
    Event.Delete(_uFuelTankDeath)
    _nFuelTanks = nil
  end
end

function _OnFuelTankDeath(uGuid)
  for sName, tEquipment in pairs(_tEquipment) do
    if tEquipment and tEquipment.uGuid == uGuid then
      local tEquipmentData = WifEquipmentData.GetEquipmentData(sName)
      if not tEquipmentData then
        return
      end
      Debug.Printf("FuelTank " .. tEquipmentData.nFuelTankId .. " died!")
      AddFuelCapacity(-tEquipmentData.nFuelCapacity)
      tEquipment.bPristine = nil
      _nFuelTanks = _nFuelTanks - 1
      if _nFuelTanks == 0 then
        Event.Delete(_uFuelTankDeath)
        _nFuelTanks = nil
      end
      return
    end
  end
  Debug.Printf("No matching FuelTank for " .. tostring(uGuid))
end

function GetClientReimburseAmount()
  local totalAmount = 0
  if not _tStockpile then
    Debug.Printf("GetClientReimburseAmount(): No stockpile, so no reimburse amount.")
    return 0
  end
  if not Net.IsClient() then
    return 0
  end
  for sId, tClientSpendings in pairs(_tClientSupportSpendings) do
    local nLeft = 0
    if _tStockpile[sId] then
      nLeft = _tStockpile[sId].nAmt
    end
    if 0 < tClientSpendings.nUnits and 0 < nLeft then
      local nRefund = 0
      if nLeft >= tClientSpendings.nUnits then
        nRefund = tClientSpendings.nSpent
      else
        local nAvgCost = tClientSpendings.nSpent / tClientSpendings.nUnits
        nRefund = nLeft * nAvgCost
      end
      totalAmount = totalAmount + nRefund
    end
  end
  Debug.Printf("GetClientReimburseAmount(): +$" .. tostring(totalAmount))
  return totalAmount
end

function NetClientReimburse()
  local totalAmount = GetClientReimburseAmount()
  if 0 < totalAmount then
    Player.AddCash(totalAmount)
    MrxGui.AddMessage({
      sText = "[green]Stockpile Reimbursement: +$" .. tostring(totalAmount),
      nDuration = 4
    })
  end
end

_oDummyWidget = nil

function _CreateHiScoreEvent()
  if _oDummyWidget then
    return
  end
  _oDummyWidget = MrxGui.Widget:new()
  MrxGui.AddWidget(_oDummyWidget)
  _oDummyWidget:SetEventHandler("UpdateLeaderboard", _HiScoreUpdated)
end

function _HiScoreUpdated(oDummyWidget, tEvent)
  Hud.EventFanfare:Commence({
    sType = "highscore",
    sText = MrxUtil.FormatMoney(tEvent.Cash)
  })
end

function SaveSingleton()
  local nCashValue = Player.GetCash()
  local nFuelValue = Player.GetFuel()
  Player.SetFuelCapacity(GetFuelCapacity())
  local tSavedEquipment = {}
  for sName, vEquipment in pairs(_tEquipment) do
    local vValue
    if type(vEquipment) ~= "table" then
      vValue = vEquipment
    else
      vValue = vEquipment.bPristine
    end
    if vValue == true then
      tSavedEquipment[sName] = true
    end
  end
  return {
    tEquipment = tSavedEquipment,
    nCash = nCashValue,
    nFuel = nFuelValue,
    tStockpile = _tStockpile,
    tFreebies = _tFreebies
  }
end

function LoadSingleton(tSaveData)
  if not tSaveData then
    return
  end
  SetFuelCapacity(Player.GetFuelCapacity())
  if tSaveData.tEquipment then
    _tEquipment = {}
    for sName, vValue in pairs(tSaveData.tEquipment) do
      AddEquipment(sName, true)
    end
  end
  if Pg.LoadIsRetry() then
    if tSaveData.nCash then
      Player.SetCash(tSaveData.nCash)
    end
    if tSaveData.nFuel then
      Player.SetFuel(tSaveData.nFuel)
    end
  end
  if tSaveData.tStockpile then
    _tStockpile = tSaveData.tStockpile
  end
  if tSaveData.tFreebies then
    _tFreebies = tSaveData.tFreebies
  end
  DisplayResources()
end
