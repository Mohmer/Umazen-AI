pragma circom 2.1.6;

include "node_modules/circomlib/circuits/sha256.circom";
include "node_modules/circomlib/circuits/comparators.circom";
include "node_modules/circomlib/circuits/poseidon.circom";

// ========================
// Core Model Verification
// ========================

template ModelIntegrity(n_layers, layer_sizes) {
    signal input model_hash; // SHA256 of initial model weights
    signal output out;

    component sha = SHA256(256);
    
    // Layer-wise weight validation
    for (var i = 0; i < n_layers; i++) {
        signal input weights[layer_sizes[i]];
        component poseidon = Poseidon(layer_sizes[i]);
        
        for (var j = 0; j < layer_sizes[i]; j++) {
            poseidon.inputs[j] <== weights[j];
        }
        
        sha.in[j] <== poseidon.out;
    }
    
    component finalCheck = IsEqual();
    finalCheck.in[0] <== sha.out;
    finalCheck.in[1] <== model_hash;
    out <== finalCheck.out;
}

// ========================
// Gradient Computation Proof
// ========================

template GradientCorrectness(batch_size, feature_size) {
    signal input input_data[batch_size][feature_size];
    signal input labels[batch_size];
    signal input weights[feature_size];
    
    signal output loss;
    signal output gradients[feature_size];

    // Matrix operations using component composition
    component dotProducts[batch_size];
    component activations[batch_size];
    component lossCalc = Poseidon(batch_size);
    
    for (var i = 0; i < batch_size; i++) {
        dotProducts[i] = DotProduct(feature_size);
        activations[i] = ReLU();

        for (var j = 0; j < feature_size; j++) {
            dotProducts[i].a[j] <== weights[j];
            dotProducts[i].b[j] <== input_data[i][j];
        }
        
        activations[i].in <== dotProducts[i].out;
        lossCalc.inputs[i] <== (activations[i].out - labels[i]) * (activations[i].out - labels[i]);
    }

    loss <== lossCalc.out / batch_size;

    // Gradient calculation using automatic differentiation
    component gradCalc[feature_size];
    for (var j = 0; j < feature_size; j++) {
        gradCalc[j] = Poseidon(batch_size);
        
        for (var i = 0; i < batch_size; i++) {
            gradCalc[j].inputs[i] <== 2 * (activations[i].out - labels[i]) * input_data[i][j];
        }
        
        gradients[j] <== gradCalc[j].out / batch_size;
    }
}

// ========================
// Training Step Verification
// ========================

template TrainingStep(n_layers, layer_sizes, batch_size, learning_rate) {
    signal input initial_model_hash;
    signal input final_model_hash;
    signal input gradients[n_layers][];
    signal input learning_rate;

    component modelCheck = ModelIntegrity(n_layers, layer_sizes);
    modelCheck.model_hash <== initial_model_hash;

    component weightUpdates[n_layers];
    for (var i = 0; i < n_layers; i++) {
        weightUpdates[i] = LayerUpdate(layer_sizes[i], learning_rate);
        weightUpdates[i].gradients <== gradients[i];
        weightUpdates[i].initial_weights <== modelCheck.weights[i];
    }

    component finalHash = SHA256(256);
    for (var i = 0; i < n_layers; i++) {
        for (var j = 0; j < layer_sizes[i]; j++) {
            finalHash.in[j] <== weightUpdates[i].out_weights[j];
        }
    }

    component hashCheck = IsEqual();
    hashCheck.in[0] <== finalHash.out;
    hashCheck.in[1] <== final_model_hash;
    hashCheck.out === 1;
}

// ========================
// Helper Components
// ========================

template DotProduct(n) {
    signal input a[n];
    signal input b[n];
    signal output out;

    component muls[n];
    component add = Poseidon(n);

    for (var i = 0; i < n; i++) {
        muls[i] = Mul();
        muls[i].a <== a[i];
        muls[i].b <== b[i];
        add.inputs[i] <== muls[i].out;
    }
    
    out <== add.out;
}

template ReLU() {
    signal input in;
    signal output out;

    component gt = GreaterThan(64);
    gt.in[0] <== in;
    gt.in[1] <== 0;
    
    out <== gt.out * in;
}

template LayerUpdate(n_weights, learning_rate) {
    signal input initial_weights[n_weights];
    signal input gradients[n_weights];
    signal output out_weights[n_weights];

    component rateCheck = LessThan(64);
    rateCheck.in[0] <== learning_rate;
    rateCheck.in[1] <== 0.1; // Prevent too large updates
    rateCheck.out === 1;

    for (var i = 0; i < n_weights; i++) {
        component update = SafeSub(64);
        update.in[0] <== initial_weights[i];
        update.in[1] <== gradients[i] * learning_rate;
        out_weights[i] <== update.out;
    }
}

// ========================
// Main Circuit Composition
// ========================

component main {
    // Public inputs
    signal input model_hash;
    signal input training_config_hash;
    
    // Private inputs
    signal input initial_weights[][];
    signal input input_data[][];
    signal input labels[];
    signal input learning_rate;

    // Public outputs
    signal output final_model_hash;
    signal output loss;

    // Model structure parameters
    var n_layers = 3;
    var layer_sizes = [784, 128, 10]; // MNIST example
    
    component training = TrainingStep(
        n_layers,
        layer_sizes,
        /*batch_size*/ 64,
        learning_rate
    );

    training.initial_model_hash <== model_hash;
    training.final_model_hash <== final_model_hash;
    
    component gradProof = GradientCorrectness(64, 784);
    gradProof.input_data <== input_data;
    gradProof.labels <== labels;
    gradProof.weights <== initial_weights[0];
    
    training.gradients <== gradProof.gradients;
    loss <== gradProof.loss;
}
