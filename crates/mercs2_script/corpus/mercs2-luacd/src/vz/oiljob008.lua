inherit("MrxTaskJobDestroySet")
_tTargetNearbyVo = {
  {
    vSequence = "Fiona-None-Freeplay-None-07"
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil08-02"
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil08-03"
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil08-03"
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil08-02"
  },
  {
    vSequence = "Fiona-None-Freeplay-None-02"
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil08-02"
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil08-03"
  }
}
_tTargetCompleteVo = {
  {
    vSequence = "Fiona-In-Mission-Job-Oil08-04",
    tRange = {1}
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil08-05",
    tRange = {
      "[",
      2,
      7,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil08-06",
    tRange = {
      "[",
      8,
      12,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil08-07",
    tRange = {13}
  }
}

function LoadAssets(self, tSaveData)
  self:_AddTarget({
    sTarget = "OilJob008b_Pristine_Objective 1",
    sPristineLayer = "Vz_State_OilJob008_B_Pristine",
    sDefenseLayer = "Vz_State_OilJob008_B_Defenses",
    sDestroyedLayer = "Vz_State_OilJob008_B_Destroyed",
    sStagingLayer = "Vz_State_OilJob008_B_Staging"
  })
  self:_AddTarget({
    sTarget = "_maracaibo_bld_corner32x32B 0x0009b1e1",
    sPristineLayer = "Vz_State_OilJob008_C_Pristine",
    sDefenseLayer = "Vz_State_OilJob008_C_Defenses",
    sDestroyedLayer = "Vz_State_OilJob008_C_Destroyed",
    sStagingLayer = "Vz_State_OilJob008_C_Staging"
  })
  self:_AddTarget({
    sTarget = "_OilJob008a_Pristine_Objective 4",
    sPristineLayer = "Vz_State_PirJob002_01_Pristine",
    sDefenseLayer = "Vz_State_PirJob002_01_Defenses",
    sDestroyedLayer = "Vz_State_PirJob002_01_Captured",
    sStagingLayer = "Vz_State_PirJob002_01_Staging"
  })
  self:_AddTarget({
    sTarget = "_OilJob008a_Pristine_Objective 5",
    sPristineLayer = "Vz_State_GurJob005_Pristine",
    sDefenseLayer = "Vz_State_GurJob005_Defenses",
    sDestroyedLayer = "Vz_State_GurJob005_Captured",
    sStagingLayer = "Vz_State_GurJob005_Staging"
  })
  self:_AddTarget({
    sTarget = "_OilJob008a_Pristine_Objective 6",
    sPristineLayer = "Vz_State_OilJob008_k_Pristine",
    sDefenseLayer = "Vz_State_OilJob008_k_Defenses",
    sDestroyedLayer = "Vz_State_OilJob008_k_Ruined",
    sStagingLayer = "Vz_State_OilJob008_k_Staging"
  })
  self:_AddTarget({
    sTarget = "_OilJob008a_Pristine_Objective 7",
    sPristineLayer = "Vz_State_OilJob008_l_Pristine",
    sDefenseLayer = "Vz_State_OilJob008_l_Defenses",
    sDestroyedLayer = "Vz_State_OilJob008_l_Ruined",
    sStagingLayer = "Vz_State_OilJob008_l_Staging"
  })
  self:_AddTarget({
    sTarget = "_OilJob008a_Pristine_Objective 8",
    sPristineLayer = "Vz_State_OilJob008_m_Pristine",
    sDefenseLayer = "Vz_State_OilJob008_m_Defenses",
    sDestroyedLayer = "Vz_State_OilJob008_m_Ruined",
    sStagingLayer = "Vz_State_OilJob008_m_Staging"
  })
  self:_AddTarget({
    sTarget = "_OilJob008_D_Pristine_Objective 1",
    sPristineLayer = "Vz_State_OilJob008_D_Pristine",
    sDefenseLayer = "Vz_State_OilJob008_D_Defenses",
    sDestroyedLayer = "Vz_State_OilJob008_D_Ruined",
    sStagingLayer = "Vz_State_OilJob008_D_Staging"
  })
  self:_AddTarget({
    sTarget = "_OilJob008_E_Pristine_Objective 1",
    sPristineLayer = "Vz_State_OilJob008_E_Pristine",
    sDefenseLayer = "Vz_State_OilJob008_E_Defenses",
    sStagingLayer = "Vz_State_OilJob008_E_Staging"
  })
  self:_AddTarget({
    sTarget = "_OilJob008_F_Pristine_Objective 1",
    sPristineLayer = "Vz_State_OilJob008_F_Pristine",
    sDefenseLayer = "Vz_State_OilJob008_F_Defenses",
    sStagingLayer = "Vz_State_OilJob008_F_Staging"
  })
  self:_AddTarget({
    sTarget = "_OilJob008_G_Pristine_Objective 1",
    sPristineLayer = "Vz_State_OilJob008_G_Pristine",
    sDefenseLayer = "Vz_State_OilJob008_G_Defenses",
    sStagingLayer = "Vz_State_OilJob008_G_Staging"
  })
  self:_AddTarget({
    sTarget = "_OilJob008_H_Pristine_Objective 1",
    sPristineLayer = "Vz_State_OilJob008_H_Pristine",
    sDefenseLayer = "Vz_State_OilJob008_H_Defenses",
    sStagingLayer = "Vz_State_OilJob008_H_Staging"
  })
  self:_AddTarget({
    sTarget = "_OilJob008_J_Pristine_Objective 1",
    sPristineLayer = "Vz_State_OilJob008_J_Pristine",
    sDefenseLayer = "Vz_State_OilJob008_J_Defenses",
    sStagingLayer = "Vz_State_OilJob008_J_Staging"
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
