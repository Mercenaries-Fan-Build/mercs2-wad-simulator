import("MrxShop")
import("MrxTransit")
import("MrxVerifyManager")
import("MrxFactionManager")
import("MrxUtil")
import("MrxPmc")
import("MrxStarterManager")
import("WifMissionData")
import("MrxAchievements")
local nCompletedToolboxes = 0
local nTotalToolbox = 100
local sFactionName = {}
sFactionName.Vza = "VZA"
sFactionName.Pmc = "PMC"
sFactionName.Pir = "PIR"
sFactionName.Oil = "OIL"
sFactionName.Gur = "GUR"
sFactionName.Civ = "CIV"
sFactionName.Chi = "CHI"
sFactionName.All = "ALL"
local tDestroyBty = {}
tDestroyBty.Vza = 0
tDestroyBty.Pmc = 0
tDestroyBty.Pir = 0
tDestroyBty.Oil = 0
tDestroyBty.Gur = 0
tDestroyBty.Civ = 0
tDestroyBty.Chi = 0
tDestroyBty.All = 0
local tDestroyBtyTotals = {}
tDestroyBtyTotals.Vza = 0
tDestroyBtyTotals.Pmc = 0
tDestroyBtyTotals.Pir = 11
tDestroyBtyTotals.Oil = 13
tDestroyBtyTotals.Gur = 13
tDestroyBtyTotals.Civ = 0
tDestroyBtyTotals.Chi = 8
tDestroyBtyTotals.All = 24
local nContractWeight = 25
local nRecruitWeight = 10
local nShopWeight = 2
local nDestroyWeight = 3
local nHVTWeight = 5
local nToolboxWeight = 1
local nLZWeight = 3
local nTotalWeight = nContractWeight + nRecruitWeight + nShopWeight + nDestroyWeight + nHVTWeight + nToolboxWeight + nLZWeight
local tTimeStamp = {}
local nOutpostCaptured = 0
local nTotalCredit = 0
local nTotalDebit = 0
local nFuelIn = 0
local nFuelOut = 0
local nDeaths = 0
local nMedevacs = 0
local nRetries = 0
local nTransit = 0
local nBestRaceTime = 0
local tListOfDebits = {}
tListOfDebits["[Generic.CopterRepair]"] = 0
tListOfDebits["[Generic.Collateral]"] = 0
tListOfDebits["[Generic.Bribes]"] = 0
tListOfDebits["[Generic.Wagers]"] = 0
tListOfDebits["[Generic.Medevacs]"] = 0
tListOfDebits["[Generic.ShopItems]"] = 0
tListOfDebits["[Garage.replacefionacar]"] = 0
tListOfDebits["[Generic.SupportDesignators.Satellite]"] = 0
local tListOfCredits = {}
tListOfCredits["[Generic.Contracts]"] = 0
tListOfCredits["[Generic.Wagers]"] = 0
tListOfCredits["[Generic.Collectibles]"] = 0
tListOfCredits["[Generic.Pickups]"] = 0
local tListOfRaces = {}
tListOfRaces.AllCon008 = 0
tListOfRaces.ChiCon008 = 0
tListOfRaces.GurCon003 = 0
tListOfRaces.OilCon005 = 0
tListOfRaces.PirCon001 = 0
tListOfRaces.PmcCon015 = 0
tListOfRaces.PmcCon016 = 0
local tFavWeapon = {}
local tFavVehicle = {}

function Activated()
  if bActivated then
    return
  end
  bActivated = true
  local tFactionAbbrevs = MrxFactionManager.GetFactionAbbrevs()
  for _, sFaction in pairs(tFactionAbbrevs) do
    sFactionName[sFaction] = MrxFactionManager.GetShortPlayerVisibleName(sFaction)
  end
end

function LoadSingleton(tSaveData)
  if tSaveData then
    tDestroyBty = tSaveData.tDestroyBty
    nCompletedToolboxes = tSaveData.nCompletedToolboxes
    MrxVerifyManager.LoadSingleton(tSaveData.oVerifyData)
    tFavWeapon = tSaveData.tFavWeapon
    tFavVehicle = tSaveData.tFavVehicle
    nTotalCredit = tSaveData.nTotalCredit
    tListOfCredits = tSaveData.tListOfCredits
    nTotalDebit = tSaveData.nTotalDebit
    tListOfDebits = tSaveData.tListOfDebits
    nOutpostCaptured = tSaveData.nOutpostCaptured
    nFuelIn = tSaveData.nFuelIn
    nFuelOut = tSaveData.nFuelOut
    if not Pg.LoadIsRetry() then
      nDeaths = tSaveData.nDeaths
      nRetries = tSaveData.nRetries
    end
    nMedevacs = tSaveData.nMedevacs
    nTransit = tSaveData.nTransit
    tListOfRaces = tSaveData.tListOfRaces
    nBestRaceTime = tSaveData.nBestRaceTime
  end
end

function SaveSingleton()
  local tSaveData = {}
  tSaveData.tDestroyBty = tDestroyBty
  tSaveData.nCompletedToolboxes = nCompletedToolboxes
  tSaveData.oVerifyData = MrxVerifyManager.SaveSingleton()
  tSaveData.tFavWeapon = tFavWeapon
  tSaveData.tFavVehicle = tFavVehicle
  tSaveData.nTotalCredit = nTotalCredit
  tSaveData.tListOfCredits = tListOfCredits
  tSaveData.nTotalDebit = nTotalDebit
  tSaveData.tListOfDebits = tListOfDebits
  tSaveData.nOutpostCaptured = nOutpostCaptured
  tSaveData.nFuelIn = nFuelIn
  tSaveData.nFuelOut = nFuelOut
  tSaveData.nDeaths = nDeaths
  tSaveData.nMedevacs = nMedevacs
  tSaveData.nRetries = nRetries
  tSaveData.nTransit = nTransit
  tSaveData.tListOfRaces = tListOfRaces
  tSaveData.nBestRaceTime = nBestRaceTime
  return tSaveData
end

function _GetTableSizeSlow(t)
  if "table" ~= type(t) then
    return 0
  end
  local n = 0
  for vIndex in pairs(t) do
    n = n + 1
  end
  return n
end

function GetTotalNumContracts()
  return WifMissionData.GetNumContracts() - 1
end

function GetTotalNumHVTs()
  return MrxVerifyManager.GetTotal() - 1
end

function GetPercentCompleted()
  local nRecruits = 0
  for sStarterName, tStarterData in pairs(MrxStarterManager.GetStarters()) do
    if tStarterData.bPmcStarter then
      nRecruits = nRecruits + 1
    end
  end
  local tFactionAbbrevs = MrxFactionManager.GetFactionAbbrevs()
  local nDestroyNumerator = 0
  local nDestroyDenominator = 0
  for _, sFaction in pairs(tFactionAbbrevs) do
    nDestroyNumerator = nDestroyNumerator + tDestroyBty[sFaction]
    nDestroyDenominator = nDestroyDenominator + tDestroyBtyTotals[sFaction]
  end
  local tFactionAbbrevs = MrxFactionManager.GetFactionAbbrevs()
  local tShopStats = {}
  for i, sFactionAbbrev in ipairs(tFactionAbbrevs) do
    tShopStats[sFactionAbbrev] = {
      nUnlocked = MrxShop.GetNumberOfUnlockedItems(sFactionAbbrev),
      nTotal = MrxShop.GetTotalNumberOfItems(sFactionAbbrev)
    }
  end
  local nAllShopsUnlocked = 0
  local nAllShopsTotal = 0
  for sFactionAbbrev, tStats in pairs(tShopStats) do
    if 0 < tStats.nTotal then
      nAllShopsUnlocked = nAllShopsUnlocked + tStats.nUnlocked
      nAllShopsTotal = nAllShopsTotal + tStats.nTotal
    end
  end
  local tHvtTotals = {
    Vza = {
      nTotal = MrxVerifyManager.GetTotalFactionVZA() or 0,
      nCompleted = MrxVerifyManager.GetCompletedVZA()
    },
    Pmc = {
      nTotal = MrxVerifyManager.GetTotalFactionPMC() or 0,
      nCompleted = MrxVerifyManager.GetCompletedPMC()
    },
    Pir = {
      nTotal = MrxVerifyManager.GetTotalFactionPIR() or 0,
      nCompleted = MrxVerifyManager.GetCompletedPIR()
    },
    Oil = {
      nTotal = MrxVerifyManager.GetTotalFactionOIL() or 0,
      nCompleted = MrxVerifyManager.GetCompletedOIL()
    },
    Gur = {
      nTotal = MrxVerifyManager.GetTotalFactionGUR() or 0,
      nCompleted = MrxVerifyManager.GetCompletedGUR()
    },
    Civ = {
      nTotal = MrxVerifyManager.GetTotalFactionCIV() or 0,
      nCompleted = MrxVerifyManager.GetCompletedCIV()
    },
    Chi = {
      nTotal = MrxVerifyManager.GetTotalFactionCHI() or 0,
      nCompleted = MrxVerifyManager.GetCompletedCHI()
    },
    All = {
      nTotal = MrxVerifyManager.GetTotalFactionALL() or 0,
      nCompleted = MrxVerifyManager.GetCompletedALL()
    }
  }
  local sToolboxes = nCompletedToolboxes .. "/" .. nTotalToolbox
  local nContractTotal = WifMissionData.GetNumCompletedContracts() / GetTotalNumContracts()
  local nRecruitTotal = nRecruits / 4
  local nShopTotal = nAllShopsUnlocked / nAllShopsTotal
  local nDestroyBtyTotal = nDestroyNumerator / nDestroyDenominator
  local nHvtTotal = MrxVerifyManager.GetCompletedTotal() / GetTotalNumHVTs()
  local nToolboxTotal = nCompletedToolboxes / nTotalToolbox
  local nLZTotal = 0
  local tUnlocked = MrxTransit.GetUnlockedLocations()
  local tUnlockable = MrxTransit.GetUnlockableLocations()
  if tUnlocked and tUnlockable then
    local nUnlocked = _GetTableSizeSlow(tUnlocked)
    local nUnlockable = _GetTableSizeSlow(tUnlockable)
    nLZTotal = nUnlocked / nUnlockable
  end
  local nTotalWeighted = (nContractTotal * nContractWeight + nRecruitTotal * nRecruitWeight + nShopTotal * nShopWeight + nDestroyBtyTotal * nDestroyWeight + nHvtTotal * nHVTWeight + nToolboxTotal * nToolboxWeight + nLZTotal * nLZWeight) / nTotalWeight
  return math.min(nTotalWeighted, 1)
end

function BuildStats(oPda)
  local sCategoryName = "[PDA.Database.Score_Progress]"
  if not bAddedProgressCategory then
    oPda:AddStatisticCategory(sCategoryName)
    Activated()
    bAddedProgressCategory = true
  end
  local sCashText = MrxUtil.FormatMoney(MrxPmc.GetCashQty())
  local sFuelText = MrxPmc.GetFuelQty() .. "/" .. MrxPmc.GetFuelCapacity()
  oPda:AddStatisticEntry(sCategoryName, "[Generic.Cash]", sCashText)
  oPda:AddStatisticEntry(sCategoryName, "[Generic.Fuel]", sFuelText)
  if not Net.IsClient() then
    local sContractComplete = WifMissionData.GetNumCompletedContracts() .. "/" .. GetTotalNumContracts()
    local nRecruits = 0
    for sStarterName, tStarterData in pairs(MrxStarterManager.GetStarters()) do
      if tStarterData.bPmcStarter then
        nRecruits = nRecruits + 1
      end
    end
    local sRecruits = nRecruits .. "/4"
    local tFactionAbbrevs = MrxFactionManager.GetFactionAbbrevs()
    local tShopStats = {}
    for i, sFactionAbbrev in ipairs(tFactionAbbrevs) do
      tShopStats[sFactionAbbrev] = {
        nUnlocked = MrxShop.GetNumberOfUnlockedItems(sFactionAbbrev),
        nTotal = MrxShop.GetTotalNumberOfItems(sFactionAbbrev)
      }
    end
    local nAllShopsUnlocked = 0
    local nAllShopsTotal = 0
    for sFactionAbbrev, tStats in pairs(tShopStats) do
      if 0 < tStats.nTotal then
        nAllShopsUnlocked = nAllShopsUnlocked + tStats.nUnlocked
        nAllShopsTotal = nAllShopsTotal + tStats.nTotal
      end
    end
    local sDestroyBty = {}
    local tFactionAbbrevs = MrxFactionManager.GetFactionAbbrevs()
    local nDestroyNumerator = 0
    local nDestroyDenominator = 0
    for _, sFaction in pairs(tFactionAbbrevs) do
      nDestroyNumerator = nDestroyNumerator + tDestroyBty[sFaction]
      nDestroyDenominator = nDestroyDenominator + tDestroyBtyTotals[sFaction]
      sDestroyBty[sFaction] = tDestroyBty[sFaction] .. "/" .. tDestroyBtyTotals[sFaction]
    end
    local sDestroyBtyTotals = nDestroyNumerator .. "/" .. nDestroyDenominator
    local sHvtTotals = MrxVerifyManager.GetCompletedTotal() .. "/" .. GetTotalNumHVTs()
    local tHvtTotals = {
      Vza = {
        nTotal = MrxVerifyManager.GetTotalFactionVZA() or 0,
        nCompleted = MrxVerifyManager.GetCompletedVZA()
      },
      Pmc = {
        nTotal = MrxVerifyManager.GetTotalFactionPMC() or 0,
        nCompleted = MrxVerifyManager.GetCompletedPMC()
      },
      Pir = {
        nTotal = MrxVerifyManager.GetTotalFactionPIR() or 0,
        nCompleted = MrxVerifyManager.GetCompletedPIR()
      },
      Oil = {
        nTotal = MrxVerifyManager.GetTotalFactionOIL() or 0,
        nCompleted = MrxVerifyManager.GetCompletedOIL()
      },
      Gur = {
        nTotal = MrxVerifyManager.GetTotalFactionGUR() or 0,
        nCompleted = MrxVerifyManager.GetCompletedGUR()
      },
      Civ = {
        nTotal = MrxVerifyManager.GetTotalFactionCIV() or 0,
        nCompleted = MrxVerifyManager.GetCompletedCIV()
      },
      Chi = {
        nTotal = MrxVerifyManager.GetTotalFactionCHI() or 0,
        nCompleted = MrxVerifyManager.GetCompletedCHI()
      },
      All = {
        nTotal = MrxVerifyManager.GetTotalFactionALL() or 0,
        nCompleted = MrxVerifyManager.GetCompletedALL()
      }
    }
    local sToolboxes = nCompletedToolboxes .. "/" .. nTotalToolbox
    local nLZTotal = 0
    local sLZs = 0
    local tUnlocked = MrxTransit.GetUnlockedLocations()
    local tUnlockable = MrxTransit.GetUnlockableLocations()
    if tUnlocked and tUnlockable then
      local nUnlocked = _GetTableSizeSlow(tUnlocked)
      local nUnlockable = _GetTableSizeSlow(tUnlockable)
      nLZTotal = nUnlocked / nUnlockable
      sLZs = tostring(nUnlocked) .. "/" .. tostring(nUnlockable)
    end
    local nContractTotal = WifMissionData.GetNumCompletedContracts() / GetTotalNumContracts()
    local nRecruitTotal = nRecruits / 4
    local nShopTotal = nAllShopsUnlocked / nAllShopsTotal
    local nDestroyBtyTotal = nDestroyNumerator / nDestroyDenominator
    local nHvtTotal = MrxVerifyManager.GetCompletedTotal() / GetTotalNumHVTs()
    local nToolboxTotal = nCompletedToolboxes / nTotalToolbox
    local nTotalWeighted = (nContractTotal * nContractWeight + nRecruitTotal * nRecruitWeight + nShopTotal * nShopWeight + nDestroyBtyTotal * nDestroyWeight + nHvtTotal * nHVTWeight + nToolboxTotal * nToolboxWeight + nLZTotal * nLZWeight) / nTotalWeight
    nTotalWeighted = math.min(nTotalWeighted, 1)
    local sPercentComplete = string.format("%.1f", nTotalWeighted * 100) .. "%"
    oPda:AddStatisticEntry(sCategoryName, "[PDA.Database.Progress.PercentComplete]", sPercentComplete)
    oPda:AddStatisticEntry(sCategoryName, "[PDA.Database.Progress.ContractsCompleted]", sContractComplete)
    oPda:AddStatisticEntry(sCategoryName, "[PDA.Database.Progress.Recruits]", sRecruits)
    oPda:AddStatisticEntry(sCategoryName, "[PDA.Database.Progress.ShopItemsUnlocked]", nAllShopsUnlocked .. "/" .. nAllShopsTotal)
    for sFactionAbbrev, tStats in pairs(tShopStats) do
      if 0 < tStats.nTotal then
        local sMessage = "\t " .. sFactionName[sFactionAbbrev]
        local sValue = tStats.nUnlocked .. "/" .. tStats.nTotal
        oPda:AddStatisticEntry(sCategoryName, sMessage, sValue)
      end
    end
    oPda:AddStatisticEntry(sCategoryName, "[PDA.Database.Progress.FactionTargetsDestroyed]", sDestroyBtyTotals)
    for sFactionAbbrev, nTotal in pairs(tDestroyBtyTotals) do
      if nTotal ~= 0 then
        local sMessage = "\t " .. sFactionName[sFactionAbbrev] .. " "
        local sValue = tDestroyBty[sFactionAbbrev] .. "/" .. nTotal
        oPda:AddStatisticEntry(sCategoryName, sMessage, sValue)
      end
    end
    oPda:AddStatisticEntry(sCategoryName, "[PDA.Database.Progress.HVTsVerified]", sHvtTotals)
    for sFactionAbbrev, tData in pairs(tHvtTotals) do
      if tData.nTotal ~= 0 then
        local sMessage = "\t " .. sFactionName[sFactionAbbrev] .. "  "
        local sValue = tData.nCompleted .. "/" .. tData.nTotal
        oPda:AddStatisticEntry(sCategoryName, sMessage, sValue)
      end
    end
    oPda:AddStatisticEntry(sCategoryName, "[PDA.Database.Progress.ToolboxesCollected]", sToolboxes)
    oPda:AddStatisticEntry(sCategoryName, "[PDA.Database.Progress.LandingZonesUnlocked]", sLZs)
  end
end

function JobDestroyPart(sFaction)
  if sFaction then
    if tDestroyBty[sFaction] then
      tDestroyBty[sFaction] = tDestroyBty[sFaction] + 1
    else
      tDestroyBty[sFaction] = 1
    end
  end
end

function CompleteToolboxPart()
  nCompletedToolboxes = nCompletedToolboxes + 1
end

function PdaStatistics(oPda)
  UpdateWeaponTime()
  UpdateVehicleTime()
  local sCategoryName = "[Generic.Statistics]"
  if not bAddedStatisticCategory then
    oPda:AddStatisticCategory(sCategoryName)
    bAddedStatisticCategory = true
  end
  if Net.IsActive() and Net.IsClient() then
    oPda:AddStatisticEntry(sCategoryName, "[SHELL.Misc.55]", " ")
  end
  local sFavWeapon = GetFavWeapon() or " "
  oPda:AddStatisticEntry(sCategoryName, "[Generic.FavWeapon]", sFavWeapon)
  local sFavVehicle = GetFavVehicle() or " "
  oPda:AddStatisticEntry(sCategoryName, "[Generic.FavVehicle]", sFavVehicle)
  oPda:AddStatisticEntry(sCategoryName, "[Generic.OutpostsCaptured]", nOutpostCaptured)
  local sCashText
  if nTotalCredit then
    sCashText = MrxUtil.FormatMoney(nTotalCredit)
  else
    sCashText = MrxUtil.FormatMoney(0)
  end
  oPda:AddStatisticEntry(sCategoryName, "[Generic.Credits]", sCashText)
  for sReason, Amt in pairs(tListOfCredits) do
    if Amt then
      sCashText = MrxUtil.FormatMoney(Amt)
    else
      sCashText = MrxUtil.FormatMoney(0)
    end
    if sReason == "[Generic.Wagers]" then
      sReason = "[Generic.Wagers] "
    end
    oPda:AddStatisticEntry(sCategoryName, "\t " .. sReason, sCashText)
  end
  if nTotalDebit then
    sCashText = "-" .. MrxUtil.FormatMoney(nTotalDebit)
  else
    sCashText = MrxUtil.FormatMoney(0)
  end
  oPda:AddStatisticEntry(sCategoryName, "[Generic.Debits]", sCashText)
  for sReason, nAmt in pairs(tListOfDebits) do
    if nAmt then
      sCashText = "-" .. MrxUtil.FormatMoney(nAmt)
    else
      sCashText = "-" .. MrxUtil.FormatMoney(0)
    end
    oPda:AddStatisticEntry(sCategoryName, "\t " .. sReason, sCashText)
  end
  oPda:AddStatisticEntry(sCategoryName, "[Generic.FuelIn]", nFuelIn)
  oPda:AddStatisticEntry(sCategoryName, "[Generic.FuelOut]", nFuelOut)
  oPda:AddStatisticEntry(sCategoryName, "[Generic.Deaths]", nDeaths)
  oPda:AddStatisticEntry(sCategoryName, "[Generic.Medevacs]", nMedevacs)
  oPda:AddStatisticEntry(sCategoryName, "[Generic.Retries]", nRetries)
  oPda:AddStatisticEntry(sCategoryName, "[Generic.Transits]", nTransit)
  oPda:AddStatisticEntry(sCategoryName, "[Generic.BestRaceTimes]", " ")
  if tListOfRaces then
    for sMission, nTime in pairs(tListOfRaces) do
      local sMissionTitle = "\t " .. WifMissionData.GetMissionTitle(sMission)
      oPda:AddStatisticEntry(sCategoryName, sMissionTitle, Junk.FormatTime(nTime))
    end
  end
end

function AddWeaponTimer()
  uStowFavWeaponTimer = Event.CreatePersistent(Event.WeaponEvent, {
    Player.GetLocalCharacter(),
    "Stow"
  }, TrackWeaponTime, {})
  uDropFavWeaponTimer = Event.CreatePersistent(Event.WeaponEvent, {
    Player.GetLocalCharacter(),
    "Drop"
  }, TrackWeaponTime, {})
  uEquipFavWeaponTimer = Event.CreatePersistent(Event.WeaponEvent, {
    Player.GetLocalCharacter(),
    "Equip"
  }, StartWeaponTime, {})
  local uCurrentWeapon = Human.Inventory.GetPrimaryWeapon(Player.GetLocalCharacter())
  if uCurrentWeapon then
    StartWeaponTime(Player.GetLocalCharacter(), uCurrentWeapon)
  end
end

function DeleteWeaponTimer()
  if uStowFavWeaponTimer then
    Event.Delete(uStowFavWeaponTimer)
    uStowFavWeaponTimer = nil
  end
  if uDropFavWeaponTimer then
    Event.Delete(uDropFavWeaponTimer)
    uDropFavWeaponTimer = nil
  end
  if uEquipFavWeaponTimer then
    Event.Delete(uEquipFavWeaponTimer)
    uEquipFavWeaponTimer = nil
  end
end

function TrackWeaponTime(uOwner, uWeapon)
  if tTimeStamp[uWeapon] then
    local nTime = Sys.TimeStampGetElapsed(tTimeStamp[uWeapon])
    if uWeapon and Object.HasLabel(uWeapon, "weapon") and nTime then
      SetFavWeaponTime(uWeapon, nTime)
    end
  end
end

function StartWeaponTime(uOwner, uWeapon)
  if tTimeStamp[uWeapon] then
    Sys.TimeStampMark(tTimeStamp[uWeapon])
  else
    tTimeStamp[uWeapon] = Sys.MainTimeStamp()
  end
end

function UpdateWeaponTime()
  local uCurrentWeapon = Human.Inventory.GetPrimaryWeapon(Player.GetLocalCharacter())
  if uCurrentWeapon then
    TrackWeaponTime(Player.GetLocalCharacter(), uCurrentWeapon)
    StartWeaponTime(Player.GetLocalCharacter(), uCurrentWeapon)
  end
end

function SetFavWeaponTime(sWeapon, nTime)
  local sFavWeapon = Object.GetLocalizedName(sWeapon)
  if tFavWeapon[sFavWeapon] then
    tFavWeapon[sFavWeapon] = tFavWeapon[sFavWeapon] + nTime
  else
    tFavWeapon[sFavWeapon] = nTime
  end
end

function GetFavWeapon()
  local sFavWeapon
  local nMostUsed = 0
  for sWeapon, nTime in pairs(tFavWeapon) do
    if nTime > nMostUsed then
      nMostUsed = nTime
      sFavWeapon = sWeapon
    end
  end
  return sFavWeapon
end

function AddVehicleTimer()
  uExitFavVehicleTimer = Event.CreatePersistent(Event.ObjectInSeat, {
    Player.GetLocalCharacter(),
    "Vehicle",
    "d",
    "x"
  }, TrackVehicleTime, {})
  uEnterFavVehicleTimer = Event.CreatePersistent(Event.ObjectInSeat, {
    Player.GetLocalCharacter(),
    "Vehicle",
    "d",
    "e"
  }, StartVehicleTime, {})
  local uCurrentVehicle = Vehicle.GetFromRider(Player.GetLocalCharacter())
  if uCurrentVehicle then
    StartVehicleTime(Player.GetLocalCharacter(), uCurrentVehicle)
  end
end

function DeleteVehicleTimer()
  if uExitFavVehicleTimer then
    Event.Delete(uExitFavVehicleTimer)
  end
  if uEnterFavVehicleTimer then
    Event.Delete(uEnterFavVehicleTimer)
  end
end

function TrackVehicleTime(uOwner, uVehicle)
  if tTimeStamp[uVehicle] then
    local nTime = Sys.TimeStampGetElapsed(tTimeStamp[uVehicle])
    if uVehicle and Object.HasLabel(uVehicle, "Vehicle") and nTime then
      SetFavVehicleTime(uVehicle, nTime)
    end
  end
end

function StartVehicleTime(uOwner, uVehicle)
  if tTimeStamp[uVehicle] then
    Sys.TimeStampMark(tTimeStamp[uVehicle])
  else
    tTimeStamp[uVehicle] = Sys.MainTimeStamp()
  end
end

function UpdateVehicleTime()
  local uCurrentVehicle = Vehicle.GetFromRider(Player.GetLocalCharacter())
  if uCurrentVehicle then
    TrackVehicleTime(Player.GetLocalCharacter(), uCurrentVehicle)
    StartVehicleTime(Player.GetLocalCharacter(), uCurrentVehicle)
  end
end

function SetFavVehicleTime(uVehicle, nTime)
  local sFavVehicle = Object.GetLocalizedName(uVehicle)
  if tFavVehicle[sFavVehicle] then
    tFavVehicle[sFavVehicle] = tFavVehicle[sFavVehicle] + nTime
  else
    tFavVehicle[sFavVehicle] = nTime
  end
end

function GetFavVehicle()
  local sFavVehicle
  local nMostUsed = 0
  for sVehicle, nTime in pairs(tFavVehicle) do
    if nTime > nMostUsed then
      nMostUsed = nTime
      sFavVehicle = sVehicle
    end
  end
  return sFavVehicle
end

function IncreaseOutpostCapturedCounter()
  nOutpostCaptured = nOutpostCaptured + 1
end

function IncreaseCreditAmount(nAmt)
  nTotalCredit = nTotalCredit + nAmt
end

function ReasonsForCredits(sReason, nAmt)
  if tListOfCredits[sReason] then
    tListOfCredits[sReason] = tListOfCredits[sReason] + nAmt
  else
    tListOfCredits[sReason] = nAmt
  end
end

function IncreaseDebitAmount(nAmt)
  nTotalDebit = nTotalDebit - nAmt
end

function ReasonsForDebits(sReason, nAmt)
  if tListOfDebits[sReason] then
    tListOfDebits[sReason] = tListOfDebits[sReason] - nAmt
  else
    tListOfDebits[sReason] = -nAmt
  end
end

function IncreaseFuelInAmount(nFuel)
  nFuelIn = nFuelIn + nFuel
end

function IncreaseFuelOutAmount(nFuel)
  nFuelOut = nFuelOut + nFuel
end

function IncreaseDeathCounter()
  nDeaths = nDeaths + 1
end

function IncreaseMedevacCounter()
  nMedevacs = nMedevacs + 1
end

function IncreaseRetriesCounter()
  nRetries = nRetries + 1
end

function IncreaseTransitCounter()
  nTransit = nTransit + 1
end

function RecordBestTime(sMission, nTime)
  if tListOfRaces[sMission] == 0 then
    tListOfRaces[sMission] = nTime
  elseif nTime < tListOfRaces[sMission] then
    tListOfRaces[sMission] = nTime
  end
end
