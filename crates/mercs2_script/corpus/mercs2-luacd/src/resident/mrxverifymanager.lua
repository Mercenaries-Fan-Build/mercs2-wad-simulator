import("MrxAchievements")
sSolanoStatus = "alive"
tTargetListStatus = {}
tTargetListStatus.All = {}
tTargetListStatus.Chi = {}
tTargetListStatus.Civ = {}
tTargetListStatus.Gur = {}
tTargetListStatus.Oil = {}
tTargetListStatus.Pir = {}
tTargetListStatus.Pmc = {}
tTargetListStatus.Vza = {}
tTargetListStatus.All.AllCon003_HVT = ""
tTargetListStatus.Chi.ChiCon003_HVT = ""
tTargetListStatus.Gur.Mendez = ""
tTargetListStatus.Pmc["PmcCon002 Blanco"] = ""
tTargetListStatus.Pmc.CarmonaTarget = ""
tTargetListStatus.Pmc.Solano = ""
tTargetListStatus.All.AllJob002_01_Target = ""
tTargetListStatus.All.AllJob002_02_Target = ""
tTargetListStatus.All.AllJob002_03_Target = ""
tTargetListStatus.All.AllJob002_04_Target = ""
tTargetListStatus.All.AllJob002_05_Target = ""
tTargetListStatus.All.AllJob010_01_Target = ""
tTargetListStatus.All.AllJob010_02_Target = ""
tTargetListStatus.All.AllJob010_03_Target = ""
tTargetListStatus.All.AllJob010_04_Target = ""
tTargetListStatus.All.AllJob010_05_Target = ""
tTargetListStatus.Chi.ChiJob002_Target_01 = ""
tTargetListStatus.Chi.ChiJob002_Target_02 = ""
tTargetListStatus.Chi.ChiJob002_Target_03 = ""
tTargetListStatus.Chi.ChiJob002_Target_04 = ""
tTargetListStatus.Chi.ChiJob002_Target_05 = ""
tTargetListStatus.Chi.ChiJob010_Target_01 = ""
tTargetListStatus.Chi.ChiJob010_Target_02 = ""
tTargetListStatus.Chi.ChiJob010_Target_03 = ""
tTargetListStatus.Chi.ChiJob010_Target_04 = ""
tTargetListStatus.Chi.ChiJob010_Target_05 = ""
tTargetListStatus.Gur.GurJob002_01_Target = ""
tTargetListStatus.Gur.GurJob002_02_Target = ""
tTargetListStatus.Gur.GurJob002_03_Target = ""
tTargetListStatus.Gur.GurJob002_04_Target = ""
tTargetListStatus.Gur.GurJob002_05_Target = ""
tTargetListStatus.Gur.GurJob012_01_Target = ""
tTargetListStatus.Gur.GurJob012_02_Target = ""
tTargetListStatus.Gur.GurJob012_03_Target = ""
tTargetListStatus.Gur.GurJob012_04_Target = ""
tTargetListStatus.Gur.GurJob012_05_Target = ""
tTargetListStatus.Oil.OilJob011_Target_01 = ""
tTargetListStatus.Oil.OilJob011_Target_02 = ""
tTargetListStatus.Oil.OilJob011_Target_03 = ""
tTargetListStatus.Oil.OilJob011_Target_04 = ""
tTargetListStatus.Oil.OilJob011_Target_05 = ""
tTargetListStatus.Oil.OilJob012_Target_01 = ""
tTargetListStatus.Oil.OilJob012_Target_02 = ""
tTargetListStatus.Oil.OilJob012_Target_03 = ""
tTargetListStatus.Oil.OilJob012_Target_04 = ""
tTargetListStatus.Oil.OilJob012_Target_05 = ""
tTargetListStatus.Pir.PirJob012_Target_01 = ""
tTargetListStatus.Pir.PirJob012_Target_02 = ""
tTargetListStatus.Pir.PirJob012_Target_03 = ""
tTargetListStatus.Pir.PirJob012_Target_04 = ""
tTargetListStatus.Pir.PirJob012_Target_05 = ""
tTargetListStatus.Pir.PirJob012_Target_06 = ""
tTargetListStatus.Pir.PirJob012_Target_07 = ""
tTargetListStatus.Pir.PirJob012_Target_08 = ""
tTargetListStatus.Pir.PirJob012_Target_09 = ""
tTargetListStatus.Pir.PirJob012_Target_10 = ""
tChangedCallback = {}
tTargetGuidList = {}
nKilled = 0
nCaptured = 0

function LoadSingleton(tSaveData)
  if not tSaveData then
    return
  end
  Debug.Printf("LoadSingleton VerifyManager")
  tTargetListStatus = tSaveData.tTargetListStatus
  sSolanoStatus = tSaveData.sSolanoStatus
end

function SaveSingleton()
  Debug.Printf("SaveSingleton VerifyManager")
  local tSaveData = {}
  tSaveData.tTargetListStatus = tTargetListStatus
  tSaveData.sSolanoStatus = sSolanoStatus
  return tSaveData
end

function Activated()
  BuildGuidList()
end

function AddTarget(sTargetName, sStatus)
  local sTargetName2
  if not tTargetGuidList then
    Debug.Printf("Building LISTTTTTT")
    BuildGuidList()
  end
  if type(sStatus) == "nil" then
    sStatus = "alive"
  end
  if type(sTargetName) == "userdata" then
    sTargetName2 = FindTargetFromGuid(sTargetName)
  end
  if type(sTargetName2) == "nil" then
    BuildGuidList()
    sTargetName2 = FindTargetFromGuid(sTargetName)
  else
    sTargetName2 = sTargetName
  end
  if type(sTargetName2) == "string" then
    local sFactionAbbrev = _FindFactionFromName(sTargetName2)
    Debug.Printf("Adding target " .. sTargetName2)
    tTargetListStatus[sFactionAbbrev][sTargetName2] = sStatus
  end
  UpdateStats()
  _CheckJusticeAchievement()
end

function UpdateTarget(sTargetName, sStatus)
  if type(sStatus) == "nil" then
    return
  end
  if type(sTargetName) == "userdata" then
    sTargetName = FindTargetFromGuid(sTargetName)
  end
  if sTargetName then
    local sFactionAbbrev = _FindFactionFromName(sTargetName)
    if sFactionAbbrev then
      tTargetListStatus[sFactionAbbrev][sTargetName] = sStatus
      UpdateStats()
      _CheckJusticeAchievement()
    end
  end
end

function _CheckJusticeAchievement()
  local bAchieved = false
  if GetCompletedTotal() == GetTotal() - 1 and (tTargetListStatus.All.AllCon003_HVT == "killed" or tTargetListStatus.All.AllCon003_HVT == "captured" or tTargetListStatus.Chi.ChiCon003_HVT == "killed" or tTargetListStatus.Chi.ChiCon003_HVT == "captured") then
    bAchieved = true
  end
  if bAchieved then
    MrxAchievements.NetGrantAchievement("ACHIEVEMENT_JUSTICE_FOR_ALL", Player.GetPrimaryPlayer())
    Debug.Printf("update target: ACHIEVEMENT UNLOCKED justice for all")
  end
end

function CheckTechnoVikingAchievement()
  local bAchieved = false
  if GetCaptured() == GetTotal() - 1 and (tTargetListStatus.All.AllCon003_HVT == "captured" or tTargetListStatus.Chi.ChiCon003_HVT == "captured") then
    bAchieved = true
  end
  return bAchieved
end

function _FindFactionFromName(sName)
  for sFactionAbbrev, tTargets in pairs(tTargetListStatus) do
    for sTarget, _ in pairs(tTargets) do
      if sTarget == sName then
        return sFactionAbbrev
      end
    end
  end
  return nil
end

function BuildGuidList()
  local oldCount = #tTargetGuidList
  for sFactionAbbrev, tTargets in pairs(tTargetListStatus) do
    for sTarget, _ in pairs(tTargets) do
      local uGuid = Pg.GetGuidByName(sTarget)
      if uGuid then
        tTargetGuidList[uGuid] = sTarget
      end
    end
  end
  local newCount = #tTargetGuidList
end

function FindTargetFromGuid(uGuid)
  if tTargetGuidList[uGuid] then
    return tTargetGuidList[uGuid]
  end
  local oldCount = #tTargetGuidList
  BuildGuidList()
  local newCount = #tTargetGuidList
  if oldCount ~= newCount then
    return FindTargetFromGuid(uGuid)
  end
  return nil
end

function AddCallback(sTargetName, fCallback, tArgs)
  if type(sTargetName) == "userdata" then
    sTargetName = FindTargetFromGuid(sTargetName)
  end
  if type(sTargetName) == "string" then
    table.insert(tChangedCallback, {
      sTargetName,
      fCallback,
      tArgs
    })
  end
end

function UpdateStats()
  local nTempKilled = 0
  local nTempCaptured = 0
  for sFactionAbbrev, tTargets in pairs(tTargetListStatus) do
    for sTarget, sStatus in pairs(tTargets) do
      if sStatus == "killed" then
        nTempKilled = nTempKilled + 1
      elseif sStatus == "captured" then
        nTempCaptured = nTempCaptured + 1
      end
    end
  end
  nKilled = nTempKilled
  nCaptured = nTempCaptured
end

function GetStatus(sTargetName)
  if type(sTargetName) == "userdata" then
    sTargetName = FindTargetFromGuid(sTargetName)
  end
  if type(sTargetName) == "string" then
    local sFactionAbbrev = _FindFactionFromName(sTargetName)
    return tTargetListStatus[sFactionAbbrev][sTargetName]
  end
  return nil
end

function GetKilled()
  UpdateStats()
  return nKilled
end

function GetCaptured()
  UpdateStats()
  return nCaptured
end

function CountCompleted(tTargets)
  local nCompletedCount = 0
  for sTarget, sStatus in pairs(tTargets) do
    if sStatus == "killed" or sStatus == "captured" then
      nCompletedCount = nCompletedCount + 1
    end
  end
  return nCompletedCount
end

function GetCompletedTotal()
  UpdateStats()
  return nKilled + nCaptured
end

function GetTotalFactionVZA()
  return 0
end

function GetCompletedVZA()
  return "0"
end

function GetTotalFactionPMC()
  local nCount = 0
  for _, _ in pairs(tTargetListStatus.Pmc) do
    nCount = nCount + 1
  end
  return nCount
end

function GetCompletedPMC()
  local tTargets = tTargetListStatus.Pmc
  return CountCompleted(tTargets)
end

function GetTotalFactionPIR()
  local nCount = 0
  for _, _ in pairs(tTargetListStatus.Pir) do
    nCount = nCount + 1
  end
  return nCount
end

function GetCompletedPIR()
  local tTargets = tTargetListStatus.Pir
  return CountCompleted(tTargets)
end

function GetTotalFactionOIL()
  local nCount = 0
  for _, _ in pairs(tTargetListStatus.Oil) do
    nCount = nCount + 1
  end
  return nCount
end

function GetCompletedOIL()
  local tTargets = tTargetListStatus.Oil
  return CountCompleted(tTargets)
end

function GetTotalFactionGUR()
  local nCount = 0
  for _, _ in pairs(tTargetListStatus.Gur) do
    nCount = nCount + 1
  end
  return nCount
end

function GetCompletedGUR()
  local tTargets = tTargetListStatus.Gur
  return CountCompleted(tTargets)
end

function GetTotalFactionCIV()
  return 0
end

function GetCompletedCIV()
  return "0"
end

function GetTotalFactionCHI()
  local nCount = 0
  for _, _ in pairs(tTargetListStatus.Chi) do
    nCount = nCount + 1
  end
  return nCount
end

function GetCompletedCHI()
  local tTargets = tTargetListStatus.Chi
  return CountCompleted(tTargets)
end

function GetTotalFactionALL()
  local nCount = 0
  for _, _ in pairs(tTargetListStatus.All) do
    nCount = nCount + 1
  end
  return nCount
end

function GetCompletedALL()
  local tTargets = tTargetListStatus.All
  return CountCompleted(tTargets)
end

nCount = 0

function GetTotal()
  if nCount == 0 then
    for sFactionAbbrev, tTargets in pairs(tTargetListStatus) do
      for sTarget, _ in pairs(tTargets) do
        nCount = nCount + 1
      end
    end
  end
  return nCount
end

function SetKilledIfNotSet(sTargetName)
  local sFaction = _FindFactionFromName(sTargetName)
  local sStatus = tTargetListStatus[sFaction][sTargetName]
  if not sStatus or sStatus == "" or sStatus == "alive" then
    tTargetListStatus[sFaction][sTargetName] = "captured"
  else
  end
  UpdateStats()
  _CheckJusticeAchievement()
end

function SetSolanoVerified()
  UpdateStats()
  if GetKilled() > 0 then
    sSolanoStatus = "killed"
  else
    sSolanoStatus = "captured"
  end
  tTargetListStatus.Pmc.Solano = sSolanoStatus
  UpdateStats()
  _CheckJusticeAchievement()
end
