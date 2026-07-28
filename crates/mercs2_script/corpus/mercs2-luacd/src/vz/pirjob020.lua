inherit("MrxTaskJobDestroySet")
_tTargetNearbyVo = {
  {
    vSequence = "Fiona-None-Freeplay-None-08",
    tRange = {
      "[",
      1,
      5,
      "]"
    }
  },
  {
    vSequence = "Fiona-None-Freeplay-None-12",
    tRange = {
      "[",
      6,
      10,
      "]"
    }
  },
  {
    vSequence = "Fiona-None-Freeplay-None-07",
    tRange = {11}
  }
}
_tTargetCompleteVo = {
  {
    vSequence = "Fiona-None-Freeplay-None-14",
    tRange = {1}
  },
  {
    vSequence = "Fiona-In-Mission-Job-Chi10-08",
    tRange = {
      "[",
      2,
      5,
      "]"
    }
  },
  {
    vSequence = "Fiona-None-Freeplay-None-15",
    tRange = {
      "[",
      6,
      10,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-Chi07-06",
    tRange = {11}
  }
}

function LoadAssets(self, tSaveData)
  self:_AddTarget({
    sTarget = "PirJob007_Objective1",
    sPristineLayer = "Vz_State_PirJob007_A_Pristine",
    sDefenseLayer = "Vz_State_PirJob007_A_Defenses",
    sDestroyedLayer = "Vz_State_PirJob007_A_Destroyed",
    sStagingLayer = "Vz_State_PirJob007_A_Staging"
  })
  self:_AddTarget({
    sTarget = "PirJob007_Objective2",
    sPristineLayer = "Vz_State_PirJob007_B_Pristine",
    sDefenseLayer = "Vz_State_PirJob007_B_Defenses",
    sDestroyedLayer = "Vz_State_PirJob007_B_Destroyed",
    sStagingLayer = "Vz_State_PirJob007_B_Staging"
  })
  self:_AddTarget({
    sTarget = "PirJob007_Objective3",
    sPristineLayer = "Vz_State_PirJob007_C_Pristine",
    sDefenseLayer = "Vz_State_PirJob007_C_Defenses",
    sDestroyedLayer = "Vz_State_PirJob007_C_Destroyed",
    sStagingLayer = "Vz_State_PirJob007_C_Staging"
  })
  self:_AddTarget({
    sTarget = "PirJob007_Objective7",
    sPristineLayer = "Vz_State_PirJob007_F_Pristine",
    sDefenseLayer = "Vz_State_PirJob007_F_Defenses",
    sDestroyedLayer = "Vz_State_PirJob007_F_Destroyed",
    sStagingLayer = "Vz_State_PirJob007_F_Staging"
  })
  self:_AddTarget({
    sTarget = "PirJob007_Objective8",
    sPristineLayer = "Vz_State_PirJob007_G_Pristine",
    sDefenseLayer = "Vz_State_PirJob007_G_Defenses",
    sDestroyedLayer = "Vz_State_PirJob007_G_Destroyed",
    sStagingLayer = "Vz_State_PirJob007_G_Staging"
  })
  self:_AddTarget({
    sTarget = "PirJob010_Target01",
    sPristineLayer = "Vz_State_PirJob010_01",
    sDefenseLayer = "Vz_State_PirJob010_01_Defenses",
    sDestroyedLayer = "Vz_State_PirJob010_01_Destroyed"
  })
  self:_AddTarget({
    sTarget = "PirJob010_Target02",
    sPristineLayer = "Vz_State_PirJob010_02",
    sDefenseLayer = "Vz_State_PirJob010_02_Defenses",
    sDestroyedLayer = "Vz_State_PirJob010_02_Destroyed"
  })
  self:_AddTarget({
    sTarget = "PirJob010_Target03",
    sPristineLayer = "Vz_State_PirJob010_03",
    sDefenseLayer = "Vz_State_PirJob010_03_Defenses",
    sDestroyedLayer = "Vz_State_PirJob010_03_Destroyed"
  })
  self:_AddTarget({
    sTarget = "PirJob011_Target01",
    sPristineLayer = "Vz_State_PirJob011_01",
    sDefenseLayer = "Vz_State_PirJob011_01_Defenses",
    sDestroyedLayer = "Vz_State_PirJob011_01_Destroyed"
  })
  self:_AddTarget({
    sTarget = "PirJob011_Target02",
    sPristineLayer = "Vz_State_PirJob011_02",
    sDefenseLayer = "Vz_State_PirJob011_02_Defenses",
    sDestroyedLayer = "Vz_State_PirJob011_02_Destroyed"
  })
  self:_AddTarget({
    sTarget = "PirJob011_Target03",
    sPristineLayer = "Vz_State_PirJob011_03",
    sDefenseLayer = "Vz_State_PirJob011_03_Defenses",
    sDestroyedLayer = "Vz_State_PirJob011_03_Destroyed"
  })
  MrxTaskJobDestroySet.LoadAssets(self, tSaveData)
end

function Activated(self)
  MrxTaskJobDestroySet.Activated(self)
  self:_SetTargetNearbyVo(_tTargetNearbyVo)
  self:_SetTargetCompleteVo(_tTargetCompleteVo)
  self:_Go()
end

function Cleanup(self)
  MrxTaskJobDestroySet.Cleanup(self)
end
