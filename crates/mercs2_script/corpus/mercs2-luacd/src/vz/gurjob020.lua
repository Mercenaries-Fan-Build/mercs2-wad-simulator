inherit("MrxTaskJobDestroySet")
import("DangerousBuilding")
_tTargetNearbyVo = {
  {
    vSequence = "Fiona-In-Mission-Job-Gur11-02"
  },
  {
    vSequence = "Fiona-In-Mission-Job-Gur11-03"
  },
  {
    vSequence = "Fiona-In-Mission-Job-Gur11-04"
  },
  {
    vSequence = "Fiona-In-Mission-Job-Gur07-06"
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil08-02"
  }
}
_tTargetCompleteVo = {
  {
    vSequence = "Fiona.xfio164",
    tRange = {
      "[",
      1,
      11,
      "]"
    }
  },
  {
    vSequence = "Fiona.xfio165",
    tRange = {
      "[",
      1,
      11,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-Gur07-07",
    tRange = {
      "[",
      1,
      11,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-Gur07-08",
    tRange = {
      "[",
      1,
      11,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-Pir07-04",
    nWeight = 2,
    tRange = {
      "[",
      3,
      6,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-Gur11-05",
    tRange = {
      "[",
      1,
      11,
      "]"
    }
  },
  {
    vSequence = "Fiona.cb2fio06",
    nWeight = 10,
    tRange = {
      "[",
      12,
      13,
      ")"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-Gur07-10",
    nWeight = 10,
    tRange = {13}
  }
}
_tBuildings = {
  "GurJob011a_Target",
  "GurJob011b_Target",
  "GurJob011c_Target",
  "GurJob011d_Target",
  "GurJob011e_Target",
  "GurJob011f_Target",
  "GurJob011g_Target",
  "GurJob011h_Target",
  "GurJob011i_Target",
  "GurJob011j_Target"
}

function LoadAssets(self, tSaveData)
  self:_AddTarget({
    sTarget = "GurJob007_Target01",
    sPristineLayer = "Vz_State_GurJob007_01",
    sDefenseLayer = "Vz_State_GurJob007_01_Defenses",
    sDestroyedLayer = "Vz_State_GurJob007_01_Destroyed"
  })
  self:_AddTarget({
    sTarget = "GurJob007_Target02",
    sPristineLayer = "Vz_State_GurJob007_02",
    sDefenseLayer = "Vz_State_GurJob007_02_Defenses",
    sDestroyedLayer = "Vz_State_GurJob007_02_Destroyed"
  })
  self:_AddTarget({
    sTarget = "GurJob007_Target03",
    sPristineLayer = "Vz_State_GurJob007_03",
    sDefenseLayer = "Vz_State_GurJob007_03_Defenses",
    sDestroyedLayer = "Vz_State_GurJob007_03_Destroyed"
  })
  self:_AddTarget({
    sTarget = "GurJob011a_Target",
    sPristineLayer = "Vz_State_GurJob11_01",
    sDefenseLayer = "Vz_State_GurJob011_01_Defenses",
    sDestroyedLayer = "Vz_State_GurJob011_01_Destroyed"
  })
  self:_AddTarget({
    sTarget = "GurJob011b_Target",
    sPristineLayer = "Vz_State_GurJob11_02",
    sDefenseLayer = "Vz_State_GurJob011_02_Defenses",
    sDestroyedLayer = "Vz_State_GurJob011_02_Destroyed"
  })
  self:_AddTarget({
    sTarget = "GurJob011c_Target",
    sPristineLayer = "Vz_State_GurJob11_03",
    sDefenseLayer = "Vz_State_GurJob011_03_Defenses",
    sDestroyedLayer = "Vz_State_GurJob011_03_Destroyed"
  })
  self:_AddTarget({
    sTarget = "GurJob011d_Target",
    sPristineLayer = "Vz_State_GurJob11_04",
    sDefenseLayer = "Vz_State_GurJob011_04_Defenses",
    sDestroyedLayer = "Vz_State_GurJob011_04_Destroyed"
  })
  self:_AddTarget({
    sTarget = "GurJob011e_Target",
    sPristineLayer = "Vz_State_GurJob11_05",
    sDefenseLayer = "Vz_State_GurJob011_05_Defenses",
    sDestroyedLayer = "Vz_State_GurJob011_05_Destroyed"
  })
  self:_AddTarget({
    sTarget = "GurJob011f_Target",
    sPristineLayer = "Vz_State_GurJob11_06",
    sDefenseLayer = "Vz_State_GurJob011_06_Defenses",
    sDestroyedLayer = "Vz_State_GurJob011_06_Destroyed"
  })
  self:_AddTarget({
    sTarget = "GurJob011g_Target",
    sPristineLayer = "Vz_State_GurJob11_07",
    sDefenseLayer = "Vz_State_GurJob011_07_Defenses",
    sDestroyedLayer = "Vz_State_GurJob011_07_Destroyed"
  })
  self:_AddTarget({
    sTarget = "GurJob011h_Target",
    sPristineLayer = "Vz_State_GurJob11_08",
    sDefenseLayer = "Vz_State_GurJob011_08_Defenses",
    sDestroyedLayer = "Vz_State_GurJob011_08_Destroyed"
  })
  self:_AddTarget({
    sTarget = "GurJob011i_Target",
    sPristineLayer = "Vz_State_GurJob11_09",
    sDefenseLayer = "Vz_State_GurJob011_09_Defenses",
    sDestroyedLayer = "Vz_State_GurJob011_09_Destroyed"
  })
  self:_AddTarget({
    sTarget = "GurJob011j_Target",
    sPristineLayer = "Vz_State_GurJob11_10",
    sDefenseLayer = "Vz_State_GurJob011_10_Defenses",
    sDestroyedLayer = "Vz_State_GurJob011_10_Destroyed"
  })
  MrxTaskJobDestroySet.LoadAssets(self, tSaveData)
end

function Activated(self)
  MrxTaskJobDestroySet.Activated(self)
  self:_SetTargetNearbyVo(_tTargetNearbyVo)
  self:_SetTargetCompleteVo(_tTargetCompleteVo)
  ActivateBuilding(self, "GurJob007_Target01")
  ActivateBuilding(self, "GurJob007_Target02")
  ActivateBuilding(self, "GurJob007_Target03")
  for i, sTargetName in ipairs(_tBuildings) do
    self:ActivateBuilding(sTargetName)
  end
  self:_Go()
end

function ActivateBuilding(self, sBuildingName)
  local uBuildingGuid = Pg.GetGuidByName(sBuildingName)
  DangerousBuilding.TurnOn(uBuildingGuid, false, true)
  Ai.TweakAttachedSpawners(uBuildingGuid, {
    SpawnerState = "on",
    SpawnList = "Spawnlist (VZ Balcony)"
  })
  Ai.TweakAttachedSpawnersInGroup(uBuildingGuid, "Ground", {
    SpawnerState = "on",
    SpawnList = "Spawnlist (VZ Ground)"
  })
end
